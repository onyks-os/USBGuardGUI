use std::time::{Duration, Instant};

use super::{NO_AGENT_HINT_AFTER, WAITING_NOTICE_AFTER, spawn};
use crate::model::{AppError, OperationOutcome, PendingKind as PendingStage, UiEvent};

#[test]
fn completed_operation_reports_its_outcome() {
    let (tx, rx) = async_channel::unbounded();
    let op = spawn("test", tx, async {
        Ok(OperationOutcome::ParameterSet {
            previous: "block".into(),
        })
    });
    let event = rx.recv_blocking().unwrap();
    assert_eq!(
        event,
        UiEvent::OperationFinished {
            op: op.id,
            result: Ok(OperationOutcome::ParameterSet {
                previous: "block".into()
            }),
        }
    );
}

#[test]
fn cancel_reports_and_invalidates() {
    let (tx, rx) = async_channel::unbounded();
    let op = spawn("slow", tx, async {
        // Stands in for a Polkit prompt left open.
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Err(AppError::Cancelled)
    });
    let id = op.id;
    op.cancel();
    assert_eq!(
        rx.recv_blocking().unwrap(),
        UiEvent::OperationFinished {
            op: id,
            result: Err(AppError::Cancelled)
        }
    );
    assert_eq!(rx.recv_blocking().unwrap(), UiEvent::InvalidateRuleCache);
}

#[test]
fn stages_never_auto_cancel() {
    let (tx, _rx) = async_channel::unbounded();
    let op = spawn("x", tx, async {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Err(AppError::Cancelled)
    });
    let t0 = op.started;
    assert_eq!(op.stage(t0), PendingStage::Started);
    assert_eq!(
        op.stage(t0 + WAITING_NOTICE_AFTER.saturating_sub(Duration::from_millis(1))),
        PendingStage::Started
    );
    assert_eq!(
        op.stage(t0 + WAITING_NOTICE_AFTER),
        PendingStage::WaitingForAuthentication
    );
    assert_eq!(
        op.stage(t0 + NO_AGENT_HINT_AFTER.saturating_sub(Duration::from_millis(1))),
        PendingStage::WaitingForAuthentication
    );
    for elapsed in [NO_AGENT_HINT_AFTER, Duration::from_secs(600)] {
        assert_eq!(op.stage(t0 + elapsed), PendingStage::PossiblyNoAgent);
    }
    assert!(!op.is_finished());
    let _ = Instant::now();
    op.cancel();
}

#[test]
fn cancellation_survives_a_full_ui_channel() {
    crate::runtime::runtime().block_on(async {
        let (tx, rx) = async_channel::bounded(1);
        tx.send(UiEvent::Reconnected).await.unwrap();
        let op = spawn("pending authentication", tx, std::future::pending());
        let id = op.id;
        op.cancel();
        assert_eq!(rx.recv().await.unwrap(), UiEvent::Reconnected);
        let finished = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
        assert_eq!(
            finished.unwrap().unwrap(),
            UiEvent::OperationFinished {
                op: id,
                result: Err(AppError::Cancelled),
            }
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap(),
            UiEvent::InvalidateRuleCache
        );
    });
}

#[test]
fn cancelling_a_completed_operation_does_not_overwrite_success() {
    let (tx, rx) = async_channel::unbounded();
    let op = spawn("done", tx, async {
        Ok(OperationOutcome::RuleAppended(crate::model::RuleId::new(2)))
    });
    let event = rx.recv_blocking().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !op.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(matches!(
        event,
        UiEvent::OperationFinished { result: Ok(_), .. }
    ));
    op.cancel();
    assert!(rx.try_recv().is_err());
}
