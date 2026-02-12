use alloy::primitives::B256;
use alloy::rpc::types::Log;
use alloy::sol_types::SolEvent;
use anyhow::Result;

use crate::contracts::{
    ConditionPreparation, MarketPrepared, OrderCancelled, OrderFilled, OrdersMatched,
    PayoutRedemption, PositionSplit, PositionsConverted, PositionsMerge, QuestionPrepared,
};

pub struct DecodedEvent {
    pub event_type: &'static str,
    pub columns: Vec<(&'static str, String)>,
}

pub fn topic0(log: &Log) -> Option<B256> {
    log.topics().first().copied()
}

pub fn value_to_string<T: serde::Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    Ok(match value {
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    })
}

pub fn decode_log(log: &Log) -> Result<Option<DecodedEvent>> {
    let Some(topic0) = topic0(log) else {
        return Ok(None);
    };

    if topic0 == ConditionPreparation::SIGNATURE_HASH {
        let event = log.log_decode::<ConditionPreparation>()?;
        return Ok(Some(DecodedEvent {
            event_type: "ConditionPreparation",
            columns: vec![
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("oracle", value_to_string(&event.inner.oracle)?),
                ("question_id", value_to_string(&event.inner.questionId)?),
                (
                    "outcome_slot_count",
                    value_to_string(&event.inner.outcomeSlotCount)?,
                ),
            ],
        }));
    }

    if topic0 == PositionSplit::SIGNATURE_HASH {
        let event = log.log_decode::<PositionSplit>()?;
        return Ok(Some(DecodedEvent {
            event_type: "PositionSplit",
            columns: vec![
                (
                    "collateral_token",
                    value_to_string(&event.inner.collateralToken)?,
                ),
                (
                    "parent_collection_id",
                    value_to_string(&event.inner.parentCollectionId)?,
                ),
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("partition", value_to_string(&event.inner.partition)?),
                ("amounts", value_to_string(&event.inner.amounts)?),
                ("amount", value_to_string(&event.inner.amount)?),
            ],
        }));
    }

    if topic0 == PositionsMerge::SIGNATURE_HASH {
        let event = log.log_decode::<PositionsMerge>()?;
        return Ok(Some(DecodedEvent {
            event_type: "PositionsMerge",
            columns: vec![
                (
                    "collateral_token",
                    value_to_string(&event.inner.collateralToken)?,
                ),
                (
                    "parent_collection_id",
                    value_to_string(&event.inner.parentCollectionId)?,
                ),
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("partition", value_to_string(&event.inner.partition)?),
                ("amounts", value_to_string(&event.inner.amounts)?),
                ("amount", value_to_string(&event.inner.amount)?),
            ],
        }));
    }

    if topic0 == PayoutRedemption::SIGNATURE_HASH {
        let event = log.log_decode::<PayoutRedemption>()?;
        return Ok(Some(DecodedEvent {
            event_type: "PayoutRedemption",
            columns: vec![
                ("redeemer", value_to_string(&event.inner.redeemer)?),
                (
                    "collateral_token",
                    value_to_string(&event.inner.collateralToken)?,
                ),
                (
                    "parent_collection_id",
                    value_to_string(&event.inner.parentCollectionId)?,
                ),
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("index_sets", value_to_string(&event.inner.indexSets)?),
                ("amount", value_to_string(&event.inner.amount)?),
            ],
        }));
    }

    if topic0 == OrderFilled::SIGNATURE_HASH {
        let event = log.log_decode::<OrderFilled>()?;
        return Ok(Some(DecodedEvent {
            event_type: "OrderFilled",
            columns: vec![
                ("order_hash", value_to_string(&event.inner.orderHash)?),
                ("maker", value_to_string(&event.inner.maker)?),
                ("taker", value_to_string(&event.inner.taker)?),
                (
                    "maker_fill_amount",
                    value_to_string(&event.inner.makerFillAmount)?,
                ),
                (
                    "taker_fill_amount",
                    value_to_string(&event.inner.takerFillAmount)?,
                ),
                ("maker_fee", value_to_string(&event.inner.makerFee)?),
                ("taker_fee", value_to_string(&event.inner.takerFee)?),
            ],
        }));
    }

    if topic0 == OrdersMatched::SIGNATURE_HASH {
        let event = log.log_decode::<OrdersMatched>()?;
        return Ok(Some(DecodedEvent {
            event_type: "OrdersMatched",
            columns: vec![
                (
                    "taker_order_hash",
                    value_to_string(&event.inner.takerOrderHash)?,
                ),
                (
                    "maker_order_hash",
                    value_to_string(&event.inner.makerOrderHash)?,
                ),
            ],
        }));
    }

    if topic0 == OrderCancelled::SIGNATURE_HASH {
        let event = log.log_decode::<OrderCancelled>()?;
        return Ok(Some(DecodedEvent {
            event_type: "OrderCancelled",
            columns: vec![("order_hash", value_to_string(&event.inner.orderHash)?)],
        }));
    }

    if topic0 == MarketPrepared::SIGNATURE_HASH {
        let event = log.log_decode::<MarketPrepared>()?;
        return Ok(Some(DecodedEvent {
            event_type: "MarketPrepared",
            columns: vec![
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("creator", value_to_string(&event.inner.creator)?),
                ("market_id", value_to_string(&event.inner.marketId)?),
                ("data", value_to_string(&event.inner.data)?),
            ],
        }));
    }

    if topic0 == QuestionPrepared::SIGNATURE_HASH {
        let event = log.log_decode::<QuestionPrepared>()?;
        return Ok(Some(DecodedEvent {
            event_type: "QuestionPrepared",
            columns: vec![
                ("question_id", value_to_string(&event.inner.questionId)?),
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("creator", value_to_string(&event.inner.creator)?),
                ("data", value_to_string(&event.inner.data)?),
            ],
        }));
    }

    if topic0 == PositionsConverted::SIGNATURE_HASH {
        let event = log.log_decode::<PositionsConverted>()?;
        return Ok(Some(DecodedEvent {
            event_type: "PositionsConverted",
            columns: vec![
                ("condition_id", value_to_string(&event.inner.conditionId)?),
                ("stakeholder", value_to_string(&event.inner.stakeholder)?),
                ("amount", value_to_string(&event.inner.amount)?),
                ("fee", value_to_string(&event.inner.fee)?),
            ],
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use alloy::primitives::{Address, B256, Bytes, Log as PrimitiveLog, LogData, U256};
    use alloy::sol_types::SolEvent;
    use serde::Serialize;

    use super::{decode_log, topic0, value_to_string};
    use crate::contracts::{
        ConditionPreparation, MarketPrepared, OrderCancelled, OrderFilled, OrdersMatched,
        PayoutRedemption, PositionSplit, PositionsConverted, PositionsMerge, QuestionPrepared,
    };

    fn test_log(topics: Vec<B256>) -> alloy::rpc::types::Log {
        alloy::rpc::types::Log {
            inner: PrimitiveLog {
                address: Address::ZERO,
                data: LogData::new_unchecked(topics, Bytes::new()),
            },
            ..Default::default()
        }
    }

    fn test_event_log(data: LogData) -> alloy::rpc::types::Log {
        alloy::rpc::types::Log {
            inner: PrimitiveLog {
                address: Address::ZERO,
                data,
            },
            ..Default::default()
        }
    }

    fn assert_decode_event(
        log: alloy::rpc::types::Log,
        expected_event_type: &str,
        expected_columns: &[&str],
    ) {
        let decoded = decode_log(&log).expect("decode").expect("event");
        assert_eq!(decoded.event_type, expected_event_type);
        let actual_columns: Vec<&str> = decoded.columns.iter().map(|(name, _)| *name).collect();
        assert_eq!(actual_columns, expected_columns);
    }

    struct AlwaysFailSerialize;

    impl Serialize for AlwaysFailSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("forced serialization failure"))
        }
    }

    #[test]
    fn topic0_returns_first_topic() {
        let first = B256::with_last_byte(1);
        let second = B256::with_last_byte(2);
        let log = test_log(vec![first, second]);

        assert_eq!(topic0(&log), Some(first));
    }

    #[test]
    fn topic0_returns_none_for_empty_topics() {
        let log = test_log(vec![]);
        assert_eq!(topic0(&log), None);
    }

    #[test]
    fn value_to_string_returns_raw_string_for_string_values() {
        let value = String::from("hello");
        let actual = value_to_string(&value).expect("string value");
        assert_eq!(actual, "hello");
    }

    #[test]
    fn value_to_string_serializes_non_string_values_to_json() {
        let value = vec![1, 2, 3];
        let actual = value_to_string(&value).expect("json value");
        assert_eq!(actual, "[1,2,3]");
    }

    #[test]
    fn value_to_string_propagates_serialization_errors() {
        let error = value_to_string(&AlwaysFailSerialize).expect_err("serialize error");
        assert!(
            error.to_string().contains("forced serialization failure"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn decode_log_returns_none_for_missing_topic0() {
        let log = test_log(vec![]);
        assert!(decode_log(&log).expect("decode").is_none());
    }

    #[test]
    fn decode_log_returns_none_for_unknown_topic() {
        let log = test_log(vec![B256::with_last_byte(0xFF)]);
        assert!(decode_log(&log).expect("decode").is_none());
    }

    #[test]
    fn decode_log_decodes_condition_preparation() {
        let event = ConditionPreparation {
            conditionId: B256::with_last_byte(1),
            oracle: Address::with_last_byte(2),
            questionId: B256::with_last_byte(3),
            outcomeSlotCount: U256::from(2u64),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "ConditionPreparation",
            &[
                "condition_id",
                "oracle",
                "question_id",
                "outcome_slot_count",
            ],
        );
    }

    #[test]
    fn decode_log_decodes_position_split() {
        let event = PositionSplit {
            collateralToken: Address::with_last_byte(1),
            parentCollectionId: Address::with_last_byte(2),
            conditionId: B256::with_last_byte(3),
            partition: B256::with_last_byte(4),
            amounts: vec![U256::from(1u64), U256::from(2u64)],
            amount: U256::from(3u64),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "PositionSplit",
            &[
                "collateral_token",
                "parent_collection_id",
                "condition_id",
                "partition",
                "amounts",
                "amount",
            ],
        );
    }

    #[test]
    fn decode_log_decodes_positions_merge() {
        let event = PositionsMerge {
            collateralToken: Address::with_last_byte(1),
            parentCollectionId: Address::with_last_byte(2),
            conditionId: B256::with_last_byte(3),
            partition: B256::with_last_byte(4),
            amounts: vec![U256::from(1u64), U256::from(2u64)],
            amount: U256::from(3u64),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "PositionsMerge",
            &[
                "collateral_token",
                "parent_collection_id",
                "condition_id",
                "partition",
                "amounts",
                "amount",
            ],
        );
    }

    #[test]
    fn decode_log_decodes_payout_redemption() {
        let event = PayoutRedemption {
            redeemer: Address::with_last_byte(1),
            collateralToken: Address::with_last_byte(2),
            parentCollectionId: B256::with_last_byte(3),
            conditionId: B256::with_last_byte(4),
            indexSets: vec![U256::from(5u64)],
            amount: U256::from(6u64),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "PayoutRedemption",
            &[
                "redeemer",
                "collateral_token",
                "parent_collection_id",
                "condition_id",
                "index_sets",
                "amount",
            ],
        );
    }

    #[test]
    fn decode_log_decodes_order_filled() {
        let event = OrderFilled {
            orderHash: B256::with_last_byte(1),
            maker: Address::with_last_byte(2),
            taker: Address::with_last_byte(3),
            makerFillAmount: U256::from(4u64),
            takerFillAmount: U256::from(5u64),
            makerFee: U256::from(6u64),
            takerFee: U256::from(7u64),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "OrderFilled",
            &[
                "order_hash",
                "maker",
                "taker",
                "maker_fill_amount",
                "taker_fill_amount",
                "maker_fee",
                "taker_fee",
            ],
        );
    }

    #[test]
    fn decode_log_decodes_orders_matched() {
        let event = OrdersMatched {
            takerOrderHash: B256::with_last_byte(1),
            makerOrderHash: B256::with_last_byte(2),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "OrdersMatched",
            &["taker_order_hash", "maker_order_hash"],
        );
    }

    #[test]
    fn decode_log_decodes_order_cancelled() {
        let event = OrderCancelled {
            orderHash: B256::with_last_byte(1),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "OrderCancelled",
            &["order_hash"],
        );
    }

    #[test]
    fn decode_log_decodes_market_prepared() {
        let event = MarketPrepared {
            conditionId: B256::with_last_byte(1),
            creator: Address::with_last_byte(2),
            marketId: U256::from(3u64),
            data: Bytes::from(vec![1_u8, 2_u8, 3_u8]),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "MarketPrepared",
            &["condition_id", "creator", "market_id", "data"],
        );
    }

    #[test]
    fn decode_log_decodes_question_prepared() {
        let event = QuestionPrepared {
            questionId: B256::with_last_byte(1),
            conditionId: B256::with_last_byte(2),
            creator: Address::with_last_byte(3),
            data: Bytes::from(vec![1_u8, 2_u8]),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "QuestionPrepared",
            &["question_id", "condition_id", "creator", "data"],
        );
    }

    #[test]
    fn decode_log_decodes_positions_converted() {
        let event = PositionsConverted {
            conditionId: B256::with_last_byte(1),
            stakeholder: Address::with_last_byte(2),
            amount: U256::from(3u64),
            fee: U256::from(4u64),
        };
        assert_decode_event(
            test_event_log(event.encode_log_data()),
            "PositionsConverted",
            &["condition_id", "stakeholder", "amount", "fee"],
        );
    }
}
