use anyhow::Result;
use katana_primitives::block::BlockHashOrNumber;
use katana_primitives::env::{BlockEnv, CfgEnv};

/// A provider that provides block environment values including Starknet execution environment
/// values.
#[auto_impl::auto_impl(&, Box, Arc)]
pub trait BlockEnvProvider: Send + Sync {
    /// Returns the block environment values at the given block id.
    fn env_at(&self, block_id: BlockHashOrNumber) -> Result<BlockEnv>;
    /// Returns the environment values at the given block id to be used in a StarknetVM execution.
    fn exec_env_at(&self, block_id: BlockHashOrNumber) -> Result<(BlockEnv, CfgEnv)>;
}
