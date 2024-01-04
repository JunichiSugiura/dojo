use std::collections::HashMap;

use crate::block::{BlockNumber, GasPrices};
use crate::chain::ChainId;
use crate::contract::ContractAddress;

/// Block environment values.
#[derive(Debug, Clone, Default)]
pub struct BlockEnv {
    /// The block height.
    pub number: BlockNumber,
    /// The timestamp in seconds since the UNIX epoch.
    pub timestamp: u64,
    /// The L1 gas prices at this particular block.
    pub gas_prices: GasPrices,
    /// The contract address of the sequencer.
    pub sequencer_address: ContractAddress,
}

/// The chain configuration values.
#[derive(Debug, Clone, Default)]
pub struct CfgEnv {
    /// The chain id.
    pub chain_id: ChainId,
    /// The contract address of the fee token.
    pub fee_token_address: ContractAddress,
    /// The fee cost of the VM resources.
    pub vm_resource_fee_cost: HashMap<String, f64>,
    /// The maximum number of steps allowed for an invoke transaction.
    pub invoke_tx_max_n_steps: u32,
    /// The maximum number of steps allowed for transaction validation.
    pub validate_max_n_steps: u32,
    /// The maximum recursion depth allowed.
    pub max_recursion_depth: usize,
}
