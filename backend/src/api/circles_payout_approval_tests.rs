//! Tests for the Circle Payout Approval Workflow (Issue #1211 / #54).
//! Illustrative tests; they may not compile until the underlying approval
//! types and functions are implemented.

use crate::api::circles::{
    CirclesState, PayoutRequest, PayoutStatus,
};
use crate::api::circle_settings::CircleSettings;
use sqlx::PgPool;
use uuid::Uuid;

fn test_pool() -> PgPool {
    unimplemented!()
}

fn test_state() -> CirclesState {
    CirclesState {
        pool: test_pool(),
        hub: crate::api::circle_events::CircleEventHub::new(16),
    }
}

#[tokio::test]
async fn approval_requirements_are_configurable() {
    let circle_id = Uuid::new_v4();
    let settings = CircleSettings {
        circle_id,
        name: "Test Circle".into(),
        description: None,
        category: "savings".into(),
        image_url: None,
        visibility: "public".into(),
        is_discoverable: true,
        member_limit: Some(10),
        allow_join_requests: true,
        invitation_expiry_hours: Some(24),
        payout_approval_threshold: Some(2),
        eligible_approver_roles: vec!["owner".into(), "admin".into(), "moderator".into()],
    };

    assert_eq!(settings.payout_approval_threshold, Some(2));
    assert!(settings
        .eligible_approver_roles
        .contains(&"owner".into()));
}

#[tokio::test]
async fn only_eligible_approvers_can_approve() {
    let state = test_state();
    let circle_id = Uuid::new_v4();
    let requester = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string();
    let non_member = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string();

    let req = PayoutRequest {
        to: "GC...".to_string(),
        amount: "100".to_string(),
        asset: "native".to_string(),
    };

    let payout_id = state
        .create_payout_request(circle_id, requester.clone(), req)
        .await
        .unwrap();

    let result = state
        .approve_payout(circle_id, payout_id, &non_member, None)
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "not_eligible_approver");
}

#[tokio::test]
async fn rejections_include_a_reason() {
    let state = test_state();
    let circle_id = Uuid::new_v4();
    let approver = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string();

    let req = PayoutRequest {
        to: "GC...".to_string(),
        amount: "100".to_string(),
        asset: "native".to_string(),
    };

    let payout_id = state
        .create_payout_request(circle_id, "requester".into(), req)
        .await
        .unwrap();

    let result = state
        .reject_payout(
            circle_id,
            payout_id,
            &approver,
            Some("insufficient funds".into()),
        )
        .await;

    assert!(result.is_ok());

    let payout = state.get_payout(circle_id, payout_id).await.unwrap();
    assert_eq!(payout.status, PayoutStatus::Rejected);
    assert_eq!(payout.rejection_reason.as_deref(), Some("insufficient funds"));
}

#[tokio::test]
async fn approval_status_is_tracked() {
    let state = test_state();
    let circle_id = Uuid::new_v4();
    let approver = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string();

    let req = PayoutRequest {
        to: "GC...".to_string(),
        amount: "100".to_string(),
        asset: "native".to_string(),
    };

    let payout_id = state
        .create_payout_request(circle_id, "requester".into(), req)
        .await
        .unwrap();

    state
        .approve_payout(circle_id, payout_id, &approver, None)
        .await
        .unwrap();

    let payout = state.get_payout(circle_id, payout_id).await.unwrap();
    assert!(payout.approvals.contains(&approver));
    assert_eq!(payout.approval_count, 1);
    assert_eq!(payout.status, PayoutStatus::Pending);
}

#[tokio::test]
async fn duplicate_approvals_are_prevented() {
    let state = test_state();
    let circle_id = Uuid::new_v4();
    let approver = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string();

    let req = PayoutRequest {
        to: "GC...".to_string(),
        amount: "100".to_string(),
        asset: "native".to_string(),
    };

    let payout_id = state
        .create_payout_request(circle_id, "requester".into(), req)
        .await
        .unwrap();

    state
        .approve_payout(circle_id, payout_id, &approver, None)
        .await
        .unwrap();

    let result = state
        .approve_payout(circle_id, payout_id, &approver, None)
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "duplicate_approval");
}

#[tokio::test]
async fn approved_requests_can_proceed_to_execution() {
    let state = test_state();
    let circle_id = Uuid::new_v4();
    let approver1 = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string();
    let approver2 = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string();

    let req = PayoutRequest {
        to: "GC...".to_string(),
        amount: "100".to_string(),
        asset: "native".to_string(),
    };

    let payout_id = state
        .create_payout_request(circle_id, "requester".into(), req)
        .await
        .unwrap();

    state
        .approve_payout(circle_id, payout_id, &approver1, None)
        .await
        .unwrap();
    state
        .approve_payout(circle_id, payout_id, &approver2, None)
        .await
        .unwrap();

    let payout = state.get_payout(circle_id, payout_id).await.unwrap();
    assert_eq!(payout.status, PayoutStatus::Approved);

    let exec_result = state.execute_payout(circle_id, payout_id).await;
    assert!(exec_result.is_ok());

    let payout = state.get_payout(circle_id, payout_id).await.unwrap();
    assert_eq!(payout.status, PayoutStatus::Executed);
}
