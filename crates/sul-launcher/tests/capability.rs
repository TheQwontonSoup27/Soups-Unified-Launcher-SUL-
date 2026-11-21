//! Public-API contract of `sul_launcher::capability`: the rule that "Supported and tested" needs a test date and a pointer
//! to evidence (it moved here from `sul-core`'s `tests/core.rs` with the module).

use sul_launcher::capability::{CapabilityEntry, Status};

#[test]
fn supported_tested_requires_evidence() {
    let mut c = CapabilityEntry::not_investigated("launch", "helldivers2");
    assert_eq!(c.status, Status::NotInvestigated);
    assert!(c.validate().is_ok());
    c.status = Status::SupportedTested;
    assert!(c.validate().is_err());
    c.test_date = Some("2026-10-22".into());
    assert!(c.validate().is_err());
    c.evidence = Some("logs/run1.jsonl".into());
    assert!(c.validate().is_ok());
}
