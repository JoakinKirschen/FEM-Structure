use chrono::{TimeZone, Utc};
use std::collections::{BTreeMap, BTreeSet};
use structural_collaboration::*;
use uuid::Uuid;

fn time(second: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 27, 12, 0, second).unwrap()
}

fn object(id: Uuid, value: i64) -> DomainObject {
    DomainObject {
        object_id: id,
        kind: "node".to_owned(),
        payload: serde_json::json!({"x": value, "y": 0, "z": 0}),
    }
}

#[test]
fn immutable_commits_reject_stale_heads() {
    let id = Uuid::new_v4();
    let mut repo = Repository::initialize(
        Uuid::new_v4(), "owner",
        ModelSnapshot { objects: BTreeMap::from([(id, object(id, 0))]) }, time(0)
    ).unwrap();
    repo.grant("owner", "engineer".to_owned(), Role::Editor).unwrap();
    let head = repo.head("main").unwrap().revision_id.clone();
    repo.commit(
        "engineer", "main", &head,
        &[ObjectChange::Put { object: object(id, 1) }],
        "Move node", time(1)
    ).unwrap();
    assert!(repo.commit(
        "engineer", "main", &head,
        &[ObjectChange::Put { object: object(id, 2) }],
        "Stale edit", time(2)
    ).is_err());
    repo.validate().unwrap();
}

#[test]
fn object_level_merge_detects_and_resolves_conflict() {
    let id = Uuid::new_v4();
    let mut repo = Repository::initialize(
        Uuid::new_v4(), "owner",
        ModelSnapshot { objects: BTreeMap::from([(id, object(id, 0))]) }, time(0)
    ).unwrap();
    let root = repo.head("main").unwrap().revision_id.clone();
    repo.create_branch("owner", "alternative", &root).unwrap();
    repo.commit("owner", "main", &root,
        &[ObjectChange::Put { object: object(id, 1) }], "Main edit", time(1)).unwrap();
    repo.commit("owner", "alternative", &root,
        &[ObjectChange::Put { object: object(id, 2) }], "Alternative edit", time(2)).unwrap();

    let plan = repo.plan_merge("main", "alternative").unwrap();
    assert_eq!(plan.conflicts.len(), 1);
    let resolutions = BTreeMap::from([(id, ConflictResolution::UseSource)]);
    let merged = repo.merge(
        "owner", "main", "alternative", &resolutions, "Resolve design", time(3)
    ).unwrap();
    assert_eq!(
        repo.revisions[&merged].snapshot.objects[&id].payload["x"],
        serde_json::json!(2)
    );
    assert_eq!(repo.revisions[&merged].parents.len(), 2);
    repo.validate().unwrap();
}


#[test]
fn independent_object_edits_merge_without_conflict() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let mut repo = Repository::initialize(
        Uuid::new_v4(), "owner",
        ModelSnapshot { objects: BTreeMap::from([(a, object(a, 0)), (b, object(b, 0))]) },
        time(0)
    ).unwrap();
    let root = repo.head("main").unwrap().revision_id.clone();
    repo.create_branch("owner", "option-b", &root).unwrap();
    repo.commit("owner", "main", &root,
        &[ObjectChange::Put { object: object(a, 10) }], "Edit A", time(1)).unwrap();
    repo.commit("owner", "option-b", &root,
        &[ObjectChange::Put { object: object(b, 20) }], "Edit B", time(2)).unwrap();

    let plan = repo.plan_merge("main", "option-b").unwrap();
    assert!(plan.conflicts.is_empty());
    let merged = repo.merge(
        "owner", "main", "option-b", &BTreeMap::new(), "Combine options", time(3)
    ).unwrap();
    let snapshot = &repo.revisions[&merged].snapshot;
    assert_eq!(snapshot.objects[&a].payload["x"], serde_json::json!(10));
    assert_eq!(snapshot.objects[&b].payload["x"], serde_json::json!(20));
}

#[test]
fn access_control_separates_view_comment_edit_and_merge() {
    let id = Uuid::new_v4();
    let mut repo = Repository::initialize(
        Uuid::new_v4(), "owner",
        ModelSnapshot { objects: BTreeMap::from([(id, object(id, 0))]) }, time(0)
    ).unwrap();
    repo.grant("owner", "reviewer".to_owned(), Role::Commenter).unwrap();
    let head = repo.head("main").unwrap().revision_id.clone();
    assert!(repo.commit(
        "reviewer", "main", &head,
        &[ObjectChange::Put { object: object(id, 1) }], "Unauthorized edit", time(1)
    ).is_err());
    let comment = repo.add_comment(
        "reviewer", &head, Some(id), "Check support condition", time(1)
    ).unwrap();
    assert!(repo.set_review_state("reviewer", comment, ReviewState::Approved).is_err());
    repo.set_review_state("owner", comment, ReviewState::Approved).unwrap();
}

#[test]
fn encrypted_offline_sync_round_trip_and_fast_forward() {
    let id = Uuid::new_v4();
    let mut origin = Repository::initialize(
        Uuid::new_v4(), "owner",
        ModelSnapshot { objects: BTreeMap::from([(id, object(id, 0))]) }, time(0)
    ).unwrap();
    origin.grant("owner", "engineer".to_owned(), Role::Editor).unwrap();
    let mut replica = origin.clone();
    let root = origin.head("main").unwrap().revision_id.clone();
    origin.commit("engineer", "main", &root,
        &[ObjectChange::Put { object: object(id, 4) }], "Offline update", time(1)).unwrap();

    let known = replica.revisions.keys().cloned().collect::<BTreeSet<_>>();
    let bundle = origin.export_sync_bundle("engineer", &known).unwrap();
    let key = [7_u8; 32];
    let encrypted = EncryptedBundle::encrypt_with_nonce(&bundle, &key, [9_u8; 24]).unwrap();
    let decrypted = encrypted.decrypt(&key).unwrap();
    let report = replica.import_sync_bundle("engineer", decrypted).unwrap();
    assert_eq!(report.imported_revisions.len(), 1);
    assert!(report.branch_conflicts.is_empty());
    assert_eq!(
        replica.head("main").unwrap().snapshot.objects[&id].payload["x"],
        serde_json::json!(4)
    );
    replica.validate().unwrap();
}

#[test]
fn divergent_offline_branches_are_reported_not_overwritten() {
    let id = Uuid::new_v4();
    let mut remote = Repository::initialize(
        Uuid::new_v4(), "owner",
        ModelSnapshot { objects: BTreeMap::from([(id, object(id, 0))]) }, time(0)
    ).unwrap();
    remote.grant("owner", "engineer".to_owned(), Role::Editor).unwrap();
    let mut local = remote.clone();
    let root = remote.head("main").unwrap().revision_id.clone();
    remote.commit("engineer", "main", &root,
        &[ObjectChange::Put { object: object(id, 1) }], "Remote", time(1)).unwrap();
    local.commit("engineer", "main", &root,
        &[ObjectChange::Put { object: object(id, 2) }], "Local", time(2)).unwrap();

    let bundle = remote.export_sync_bundle("engineer", &BTreeSet::new()).unwrap();
    let local_head = local.head("main").unwrap().revision_id.clone();
    let report = local.import_sync_bundle("engineer", bundle).unwrap();
    assert_eq!(report.branch_conflicts.len(), 1);
    assert_eq!(local.head("main").unwrap().revision_id, local_head);
}
