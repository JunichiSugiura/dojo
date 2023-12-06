//! MDBX backend for the database.

pub mod cursor;
pub mod tx;

use std::path::Path;

use libmdbx::{DatabaseFlags, EnvironmentFlags, Geometry, Mode, PageSize, SyncMode, RO, RW};

use self::tx::Tx;
use crate::error::DatabaseError;
use crate::tables::{TableType, Tables};
use crate::utils;

const GIGABYTE: usize = 1024 * 1024 * 1024;
const TERABYTE: usize = GIGABYTE * 1024;

/// MDBX allows up to 32767 readers (`MDBX_READERS_LIMIT`), but we limit it to slightly below that
const DEFAULT_MAX_READERS: u64 = 32_000;

/// Environment used when opening a MDBX environment. RO/RW.
#[derive(Debug)]
pub enum DbEnvKind {
    /// Read-only MDBX environment.
    RO,
    /// Read-write MDBX environment.
    RW,
}

/// Wrapper for `libmdbx-sys` environment.
#[derive(Debug)]
pub struct DbEnv(libmdbx::Environment);

impl DbEnv {
    /// Opens the database at the specified path with the given `EnvKind`.
    ///
    /// It does not create the tables, for that call [`DbEnv::create_tables`].
    pub fn open(path: &Path, kind: DbEnvKind) -> Result<DbEnv, DatabaseError> {
        let mode = match kind {
            DbEnvKind::RO => Mode::ReadOnly,
            DbEnvKind::RW => Mode::ReadWrite { sync_mode: SyncMode::Durable },
        };

        let mut builder = libmdbx::Environment::builder();
        builder
            .set_max_dbs(Tables::ALL.len())
            .set_geometry(Geometry {
                // Maximum database size of 1 terabytes
                size: Some(0..(TERABYTE)),
                // We grow the database in increments of 4 gigabytes
                growth_step: Some(4 * GIGABYTE as isize),
                // The database never shrinks
                shrink_threshold: None,
                page_size: Some(PageSize::Set(utils::default_page_size())),
            })
            .set_flags(EnvironmentFlags {
                mode,
                // We disable readahead because it improves performance for linear scans, but
                // worsens it for random access (which is our access pattern outside of sync)
                no_rdahead: true,
                coalesce: true,
                ..Default::default()
            })
            .set_max_readers(DEFAULT_MAX_READERS);

        Ok(DbEnv(builder.open(path).map_err(DatabaseError::OpenEnv)?))
    }

    /// Creates all the defined tables in [`Tables`], if necessary.
    pub fn create_tables(&self) -> Result<(), DatabaseError> {
        let tx = self.0.begin_rw_txn().map_err(DatabaseError::CreateRWTx)?;

        for table in Tables::ALL {
            let flags = match table.table_type() {
                TableType::Table => DatabaseFlags::default(),
                TableType::DupSort => DatabaseFlags::DUP_SORT,
            };

            tx.create_db(Some(table.name()), flags).map_err(DatabaseError::CreateTable)?;
        }

        tx.commit().map_err(DatabaseError::Commit)?;

        Ok(())
    }
}

impl DbEnv {
    /// Begin a read-only transaction.
    pub fn tx(&self) -> Result<Tx<RO>, DatabaseError> {
        Ok(Tx::new(self.0.begin_ro_txn().map_err(DatabaseError::CreateROTx)?))
    }

    /// Begin a read-write transaction.
    pub fn tx_mut(&self) -> Result<Tx<RW>, DatabaseError> {
        Ok(Tx::new(self.0.begin_rw_txn().map_err(DatabaseError::CreateRWTx)?))
    }
}

// test ported from `reth`: https://github.com/paradigmxyz/reth/blob/227e1b7ad513977f4f48b18041df02686fca5f94/crates/storage/db/src/implementation/mdbx/mod.rs#L198
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use katana_primitives::block::Header;
    use katana_primitives::FieldElement;

    use super::*;
    use crate::codecs::Encode;
    use crate::mdbx::cursor::Walker;
    use crate::tables::{BlockHashes, Headers, Table};

    /// Create database for testing
    fn create_test_db(kind: DbEnvKind) -> Arc<DbEnv> {
        Arc::new(create_test_db_with_path(
            kind,
            &tempfile::TempDir::new().expect("Failed to create temp dir.").into_path(),
        ))
    }

    /// Create database for testing with specified path
    fn create_test_db_with_path(kind: DbEnvKind, path: &Path) -> DbEnv {
        let env = DbEnv::open(path, kind).expect(ERROR_DB_CREATION);
        env.create_tables().expect("Failed to create tables.");
        env
    }

    const ERROR_DB_CREATION: &str = "Not able to create the mdbx file.";
    const ERROR_PUT: &str = "Not able to insert value into table.";
    const ERROR_DELETE: &str = "Failed to delete value from table.";
    const ERROR_GET: &str = "Not able to get value from table.";
    const ERROR_COMMIT: &str = "Not able to commit transaction.";
    const ERROR_RETURN_VALUE: &str = "Mismatching result.";
    const ERROR_INIT_TX: &str = "Failed to create a MDBX transaction.";
    const ERROR_INIT_CURSOR: &str = "Failed to create cursor.";
    const ERROR_GET_AT_CURSOR_POS: &str = "Failed to get value at cursor position.";

    #[test]
    fn db_creation() {
        create_test_db(DbEnvKind::RW);
    }

    #[test]
    fn db_manual_put_get() {
        let env = create_test_db(DbEnvKind::RW);

        let value = Header::default();
        let key = 1u64;

        // PUT
        let tx = env.tx_mut().expect(ERROR_INIT_TX);
        tx.put::<Headers>(key, value.clone()).expect(ERROR_PUT);
        tx.commit().expect(ERROR_COMMIT);

        // GET
        let tx = env.tx().expect(ERROR_INIT_TX);
        let result = tx.get::<Headers>(key).expect(ERROR_GET);
        let total_entries = tx.entries::<Headers>().expect(ERROR_GET);
        tx.commit().expect(ERROR_COMMIT);

        assert!(total_entries == 1);
        assert!(result.expect(ERROR_RETURN_VALUE) == value);
    }

    #[test]
    fn db_delete() {
        let env = create_test_db(DbEnvKind::RW);

        let value = Header::default();
        let key = 1u64;

        // PUT
        let tx = env.tx_mut().expect(ERROR_INIT_TX);
        tx.put::<Headers>(key, value).expect(ERROR_PUT);
        tx.commit().expect(ERROR_COMMIT);

        let entries = env.tx().expect(ERROR_INIT_TX).entries::<Headers>().expect(ERROR_GET);
        assert!(entries == 1);

        // DELETE
        let tx = env.tx_mut().expect(ERROR_INIT_TX);
        tx.delete::<Headers>(key, None).expect(ERROR_DELETE);
        tx.commit().expect(ERROR_COMMIT);

        let entries = env.tx().expect(ERROR_INIT_TX).entries::<Headers>().expect(ERROR_GET);
        assert!(entries == 0);
    }

    #[test]
    fn db_manual_cursor_walk() {
        let env = create_test_db(DbEnvKind::RW);

        let key1 = 1u64;
        let key2 = 2u64;
        let key3 = 3u64;
        let header1 = Header::default();
        let header2 = Header::default();
        let header3 = Header::default();

        // PUT
        let tx = env.tx_mut().expect(ERROR_INIT_TX);
        tx.put::<Headers>(key1, header1.clone()).expect(ERROR_PUT);
        tx.put::<Headers>(key2, header2.clone()).expect(ERROR_PUT);
        tx.put::<Headers>(key3, header3.clone()).expect(ERROR_PUT);
        tx.commit().expect(ERROR_COMMIT);

        // CURSOR
        let tx = env.tx().expect(ERROR_INIT_TX);
        let mut cursor = tx.cursor::<Headers>().expect(ERROR_INIT_CURSOR);
        let (_, result1) = cursor.next().expect(ERROR_GET_AT_CURSOR_POS).expect(ERROR_RETURN_VALUE);
        let (_, result2) = cursor.next().expect(ERROR_GET_AT_CURSOR_POS).expect(ERROR_RETURN_VALUE);
        let (_, result3) = cursor.next().expect(ERROR_GET_AT_CURSOR_POS).expect(ERROR_RETURN_VALUE);
        tx.commit().expect(ERROR_COMMIT);

        assert!(result1 == header1);
        assert!(result2 == header2);
        assert!(result3 == header3);
    }

    #[test]
    fn db_cursor_walk() {
        let env = create_test_db(DbEnvKind::RW);

        let value = Header::default();
        let key = 1u64;

        // PUT
        let tx = env.tx_mut().expect(ERROR_INIT_TX);
        tx.put::<Headers>(key, value.clone()).expect(ERROR_PUT);
        tx.commit().expect(ERROR_COMMIT);

        // Cursor
        let tx = env.tx().expect(ERROR_INIT_TX);
        let mut cursor = tx.cursor::<Headers>().expect(ERROR_INIT_CURSOR);

        let first = cursor.first().unwrap();
        assert!(first.is_some(), "First should be our put");

        // Walk
        let walk = cursor.walk(Some(key)).unwrap();
        let first = walk.into_iter().next().unwrap().unwrap();
        assert_eq!(first.1, value, "First next should be put value");
    }

    #[test]
    fn db_walker() {
        let db = create_test_db(DbEnvKind::RW);

        // PUT (0, 0), (1, 0), (2, 0)
        let tx = db.tx_mut().expect(ERROR_INIT_TX);
        (0..3).try_for_each(|key| tx.put::<BlockHashes>(key, FieldElement::ZERO)).expect(ERROR_PUT);
        tx.commit().expect(ERROR_COMMIT);

        let tx = db.tx().expect(ERROR_INIT_TX);
        let mut cursor = tx.cursor::<BlockHashes>().expect(ERROR_INIT_CURSOR);
        let mut walker = Walker::new(&mut cursor, None);

        assert_eq!(walker.next(), Some(Ok((0, FieldElement::ZERO))));
        assert_eq!(walker.next(), Some(Ok((1, FieldElement::ZERO))));
        assert_eq!(walker.next(), Some(Ok((2, FieldElement::ZERO))));
        assert_eq!(walker.next(), None);
    }

    #[test]
    fn db_cursor_insert() {
        let db = create_test_db(DbEnvKind::RW);

        // PUT
        let tx = db.tx_mut().expect(ERROR_INIT_TX);
        (0..=4)
            .try_for_each(|key| tx.put::<BlockHashes>(key, FieldElement::ZERO))
            .expect(ERROR_PUT);
        tx.commit().expect(ERROR_COMMIT);

        let key_to_insert = 5;
        let tx = db.tx_mut().expect(ERROR_INIT_TX);
        let mut cursor = tx.cursor::<BlockHashes>().expect(ERROR_INIT_CURSOR);

        // INSERT
        assert_eq!(cursor.insert(key_to_insert, FieldElement::ZERO), Ok(()));
        assert_eq!(cursor.current(), Ok(Some((key_to_insert, FieldElement::ZERO))));

        // INSERT (failure)
        assert_eq!(
            cursor.insert(key_to_insert, FieldElement::ZERO),
            Err(DatabaseError::Write {
                table: BlockHashes::NAME,
                error: libmdbx::Error::KeyExist,
                key: key_to_insert.encode().into_boxed_slice()
            })
        );
        assert_eq!(cursor.current(), Ok(Some((key_to_insert, FieldElement::ZERO))));

        tx.commit().expect(ERROR_COMMIT);

        // Confirm the result
        let tx = db.tx().expect(ERROR_INIT_TX);
        let mut cursor = tx.cursor::<BlockHashes>().expect(ERROR_INIT_CURSOR);
        let res = cursor.walk(None).unwrap().map(|res| res.unwrap().0).collect::<Vec<_>>();
        assert_eq!(res, vec![0, 1, 2, 3, 4, 5]);
        tx.commit().expect(ERROR_COMMIT);
    }
}
