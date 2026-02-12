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
