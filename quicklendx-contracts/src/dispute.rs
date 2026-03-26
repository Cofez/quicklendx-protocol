use crate::QuickLendXError;
use crate::invoice::{Invoice, InvoiceStatus};
use crate::verification::{validate_dispute_evidence, validate_dispute_reason, validate_dispute_resolution};
use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    UnderReview,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub invoice_id: BytesN<32>,
    pub creator: Address,
    pub reason: String,
    pub evidence: String,
    pub status: DisputeStatus,
    pub resolution: Option<String>,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}





/// @notice Create a new dispute with bounded `reason` and `evidence`.
/// @dev Validation occurs before any persistent storage writes to prevent abusive
///      storage growth. The caller must be the invoice business or funding investor.
///
/// # Errors
/// - `DisputeAlreadyExists` if a dispute exists for `invoice_id`
/// - `InvoiceNotFound` if the invoice does not exist
/// - `InvoiceNotAvailableForFunding` if the invoice is not eligible
/// - `DisputeNotAuthorized` if `creator` is not an authorized invoice participant
/// - `InvalidDisputeReason` if `reason` is empty or exceeds `MAX_DISPUTE_REASON_LENGTH`
/// - `InvalidDisputeEvidence` if `evidence` is empty or exceeds `MAX_DISPUTE_EVIDENCE_LENGTH`
#[allow(dead_code)]
pub fn create_dispute(
    env: Env,
    invoice_id: BytesN<32>,
    creator: Address,
    reason: String,
    evidence: String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();

    if env.storage().persistent().has(&("dispute", invoice_id.clone())) {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    let invoice: Invoice = env
        .storage()
        .instance()
        .get(&invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    match invoice.status {
        InvoiceStatus::Pending | InvoiceStatus::Verified | InvoiceStatus::Funded | InvoiceStatus::Paid => {}
        _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
    }

    let is_authorized = creator == invoice.business || 
        invoice.investor.as_ref().map_or(false, |inv| creator == *inv);

    if !is_authorized {
        return Err(QuickLendXError::DisputeNotAuthorized);
    }

    // Validate payload sizes before persisting the dispute record.
    validate_dispute_reason(&reason)?;
    validate_dispute_evidence(&evidence)?;

    let dispute = Dispute {
        invoice_id: invoice_id.clone(),
        creator: creator.clone(),
        reason,
        evidence,
        status: DisputeStatus::Open,
        resolution: None,
        created_at: env.ledger().timestamp(),
        resolved_at: None,
    };

    env.storage()
        .persistent()
        .set(&("dispute", invoice_id), &dispute);

    Ok(())
}

#[allow(dead_code)]
pub fn put_dispute_under_review(
    env: &Env,
    admin: Address,
    invoice_id: BytesN<32>,
) -> Result<(), QuickLendXError> {
    admin.require_auth();

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&"admin")
        .ok_or(QuickLendXError::NotAdmin)?;

    if admin != stored_admin {
        return Err(QuickLendXError::Unauthorized);
    }

    let mut dispute: Dispute = env
        .storage()
        .persistent()
        .get(&("dispute", invoice_id.clone()))
        .ok_or(QuickLendXError::DisputeNotFound)?;

    if dispute.status != DisputeStatus::Open {
        return Err(QuickLendXError::InvalidStatus);
    }

    dispute.status = DisputeStatus::UnderReview;

    env.storage()
        .persistent()
        .set(&("dispute", invoice_id), &dispute);

    Ok(())
}

/// @notice Resolve an existing dispute with bounded `resolution` text.
/// @dev Resolution validation rejects empty payloads and enforces the protocol maximum
///      size to prevent storage abuse.
///
/// # Errors
/// - `DisputeNotFound` if the dispute does not exist
/// - `NotAdmin` / `Unauthorized` if the caller is not the configured admin
/// - `DisputeNotUnderReview` if the dispute is not currently under review
/// - `DisputeAlreadyResolved` if the dispute was already resolved
/// - `InvalidDisputeReason` if `resolution` is empty or exceeds `MAX_DISPUTE_RESOLUTION_LENGTH`
#[allow(dead_code)]
pub fn resolve_dispute(
    env: &Env,
    admin: Address,
    invoice_id: BytesN<32>,
    resolution: String,
) -> Result<(), QuickLendXError> {
    admin.require_auth();

    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&"admin")
        .ok_or(QuickLendXError::NotAdmin)?;

    if admin != stored_admin {
        return Err(QuickLendXError::Unauthorized);
    }

    let mut dispute: Dispute = env
        .storage()
        .persistent()
        .get(&("dispute", invoice_id.clone()))
        .ok_or(QuickLendXError::DisputeNotFound)?;

    if dispute.status != DisputeStatus::UnderReview {
        return Err(QuickLendXError::DisputeNotUnderReview);
    }

    if dispute.status == DisputeStatus::Resolved {
        return Err(QuickLendXError::DisputeAlreadyResolved);
    }

    validate_dispute_resolution(&resolution)?;

    dispute.status = DisputeStatus::Resolved;
    dispute.resolution = Some(resolution);
    dispute.resolved_at = Some(env.ledger().timestamp());

    env.storage()
        .persistent()
        .set(&("dispute", invoice_id), &dispute);

    Ok(())
}

#[allow(dead_code)]
pub fn get_dispute_details(env: &Env, invoice_id: BytesN<32>) -> Result<Dispute, QuickLendXError> {
    env.storage()
        .persistent()
        .get(&("dispute", invoice_id))
        .ok_or(QuickLendXError::DisputeNotFound)
}

#[allow(dead_code)]
pub fn get_disputes_by_status(
    env: &Env,
    status: DisputeStatus,
    start: u64,
    limit: u32,
) -> Vec<Dispute> {
    let mut disputes = Vec::new(env);
    let max_limit = 50u32;
    let query_limit = if limit > max_limit { max_limit } else { limit };

    let end = start.saturating_add(query_limit as u64);
    for i in start..end {
        if let Some(dispute) = env
            .storage()
            .persistent()
            .get::<_, Dispute>(&("dispute", i))
        {
            if dispute.status == status {
                disputes.push_back(dispute);
            }
        }
    }

    disputes
}
