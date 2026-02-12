use alloy::primitives::Address;
use alloy::sol;
use anyhow::Result;
use std::str::FromStr;

sol! {
    #[derive(Debug, serde::Serialize)]
    event ConditionPreparation(bytes32 indexed conditionId, address indexed oracle, bytes32 indexed questionId, uint256 outcomeSlotCount);

    #[derive(Debug, serde::Serialize)]
    event PositionSplit(address indexed collateralToken, address indexed parentCollectionId, bytes32 indexed conditionId, bytes32 partition, uint256[] amounts, uint256 amount);

    #[derive(Debug, serde::Serialize)]
    event PositionsMerge(address indexed collateralToken, address indexed parentCollectionId, bytes32 indexed conditionId, bytes32 partition, uint256[] amounts, uint256 amount);

    #[derive(Debug, serde::Serialize)]
    event PayoutRedemption(address indexed redeemer, address indexed collateralToken, bytes32 indexed parentCollectionId, bytes32 conditionId, uint256[] indexSets, uint256 amount);

    #[derive(Debug, serde::Serialize)]
    event OrderFilled(bytes32 indexed orderHash, address indexed maker, address indexed taker, uint256 makerFillAmount, uint256 takerFillAmount, uint256 makerFee, uint256 takerFee);

    #[derive(Debug, serde::Serialize)]
    event OrdersMatched(bytes32 indexed takerOrderHash, bytes32 indexed makerOrderHash);

    #[derive(Debug, serde::Serialize)]
    event OrderCancelled(bytes32 indexed orderHash);

    #[derive(Debug, serde::Serialize)]
    event MarketPrepared(bytes32 indexed conditionId, address indexed creator, uint256 indexed marketId, bytes data);

    #[derive(Debug, serde::Serialize)]
    event QuestionPrepared(bytes32 indexed questionId, bytes32 indexed conditionId, address indexed creator, bytes data);

    #[derive(Debug, serde::Serialize)]
    event PositionsConverted(bytes32 indexed conditionId, address indexed stakeholder, uint256 amount, uint256 fee);
}

pub const CTF_ADDRESS: &str = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045";
pub const CTF_EXCHANGE_ADDRESS: &str = "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E";
pub const NEG_RISK_ADAPTER_ADDRESS: &str = "0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296";

pub fn contract_addresses() -> Result<Vec<Address>> {
    Ok(vec![
        Address::from_str(CTF_ADDRESS)?,
        Address::from_str(CTF_EXCHANGE_ADDRESS)?,
        Address::from_str(NEG_RISK_ADAPTER_ADDRESS)?,
    ])
}

#[cfg(test)]
mod tests {
    use alloy::primitives::Address;
    use std::str::FromStr;

    use super::{CTF_ADDRESS, CTF_EXCHANGE_ADDRESS, NEG_RISK_ADAPTER_ADDRESS, contract_addresses};

    #[test]
    fn contract_addresses_returns_expected_constants_in_order() {
        let addresses = contract_addresses().expect("addresses");
        assert_eq!(addresses.len(), 3);
        assert_eq!(addresses[0], Address::from_str(CTF_ADDRESS).expect("ctf"));
        assert_eq!(
            addresses[1],
            Address::from_str(CTF_EXCHANGE_ADDRESS).expect("ctf exchange")
        );
        assert_eq!(
            addresses[2],
            Address::from_str(NEG_RISK_ADAPTER_ADDRESS).expect("neg risk adapter")
        );
    }
}
