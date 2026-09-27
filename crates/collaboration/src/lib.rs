//! Immutable model revisions, object-level merging, reviews, ACLs and offline sync.
//!
//! This crate is transport-neutral. A desktop client, on-premise server or cloud
//! service may persist and exchange the same signed/hash-addressed representations.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use structural_audit::{canonical_json_bytes, sha256_hex};
use uuid::Uuid;

pub const COLLABORATION_SCHEMA_VERSION: &str = "structural-collaboration/1.0";
pub const SYNC_SCHEMA_VERSION: &str = "structural-collaboration-sync/1.0";
pub const ENCRYPTED_BUNDLE_SCHEMA_VERSION: &str = "structural-encrypted-bundle/1.0";

pub type RevisionId = String;
pub type ObjectId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainObject {
    pub object_id: ObjectId,
    pub kind: String,
    pub payload: Value,
}

impl DomainObject {
    pub fn validate(&self) -> Result<()> {
        if self.kind.trim().is_empty() {
            bail!("domain object kind is required");
        }
        if !self.payload.is_object() {
            bail!("domain object payload must be a JSON object");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelSnapshot {
    pub objects: BTreeMap<ObjectId, DomainObject>,
}

impl ModelSnapshot {
    pub fn validate(&self) -> Result<()> {
        for (key, object) in &self.objects {
            if key != &object.object_id {
                bail!("snapshot key does not match object_id {}", object.object_id);
            }
            object.validate()?;
        }
        Ok(())
    }

    pub fn sha256(&self) -> Result<String> {
        self.validate()?;
        Ok(sha256_hex(&canonical_json_bytes(self)?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ObjectChange {
    Put { object: DomainObject },
    Delete { object_id: ObjectId },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRevision {
    pub schema_version: String,
    pub revision_id: RevisionId,
    pub parents: Vec<RevisionId>,
    pub author: String,
    pub created_at_utc: DateTime<Utc>,
    pub message: String,
    pub snapshot: ModelSnapshot,
    pub snapshot_sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Commenter,
    Editor,
    Maintainer,
    Owner,
}

impl Role {
    fn level(&self) -> u8 {
        match self {
            Self::Viewer => 0,
            Self::Commenter => 1,
            Self::Editor => 2,
            Self::Maintainer => 3,
            Self::Owner => 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessControl {
    pub principal_roles: BTreeMap<String, Role>,
}

impl AccessControl {
    pub fn require(&self, principal: &str, minimum: Role) -> Result<()> {
        let actual = self
            .principal_roles
            .get(principal)
            .with_context(|| format!("principal '{principal}' has no repository access"))?;
        if actual.level() < minimum.level() {
            bail!(
                "principal '{principal}' requires {:?}, but has {:?}",
                minimum,
                actual
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Open,
    Approved,
    ChangesRequested,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReviewComment {
    pub comment_id: Uuid,
    pub revision_id: RevisionId,
    pub object_id: Option<ObjectId>,
    pub author: String,
    pub created_at_utc: DateTime<Utc>,
    pub body: String,
    pub state: ReviewState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Branch {
    pub name: String,
    pub head: RevisionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repository {
    pub schema_version: String,
    pub repository_id: Uuid,
    pub revisions: BTreeMap<RevisionId, ModelRevision>,
    pub branches: BTreeMap<String, Branch>,
    pub comments: BTreeMap<Uuid, ReviewComment>,
    pub access: AccessControl,
}

fn validate_name(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} must not be empty");
    }
    if value.len() > 128 {
        bail!("{label} is too long");
    }
    Ok(())
}

fn revision_material(
    parents: &[RevisionId],
    author: &str,
    created_at_utc: &DateTime<Utc>,
    message: &str,
    snapshot_sha256: &str,
) -> Result<Vec<u8>> {
    canonical_json_bytes(&serde_json::json!({
        "schema_version": COLLABORATION_SCHEMA_VERSION,
        "parents": parents,
        "author": author,
        "created_at_utc": created_at_utc,
        "message": message,
        "snapshot_sha256": snapshot_sha256
    }))
}

fn make_revision(
    parents: Vec<RevisionId>,
    author: String,
    created_at_utc: DateTime<Utc>,
    message: String,
    snapshot: ModelSnapshot,
) -> Result<ModelRevision> {
    validate_name(&author, "author")?;
    validate_name(&message, "message")?;
    snapshot.validate()?;
    let snapshot_sha256 = snapshot.sha256()?;
    let revision_id = sha256_hex(&revision_material(
        &parents,
        &author,
        &created_at_utc,
        &message,
        &snapshot_sha256,
    )?);
    Ok(ModelRevision {
        schema_version: COLLABORATION_SCHEMA_VERSION.to_owned(),
        revision_id,
        parents,
        author,
        created_at_utc,
        message,
        snapshot,
        snapshot_sha256,
    })
}

fn validate_revision(revision: &ModelRevision, map_key: Option<&str>) -> Result<()> {
    if revision.schema_version != COLLABORATION_SCHEMA_VERSION {
        bail!("unsupported revision schema '{}'", revision.schema_version);
    }
    if let Some(map_key) = map_key {
        if map_key != revision.revision_id.as_str() {
            bail!("revision map key mismatch");
        }
    }
    validate_name(&revision.author, "revision author")?;
    validate_name(&revision.message, "revision message")?;
    if revision.parents.len() > 2 {
        bail!("revision {} has more than two parents", revision.revision_id);
    }
    let unique_parents: BTreeSet<_> = revision.parents.iter().collect();
    if unique_parents.len() != revision.parents.len() {
        bail!("revision {} contains duplicate parents", revision.revision_id);
    }
    if revision
        .parents
        .iter()
        .any(|parent| parent == &revision.revision_id)
    {
        bail!("revision {} cannot be its own parent", revision.revision_id);
    }
    if revision.snapshot.sha256()? != revision.snapshot_sha256 {
        bail!(
            "snapshot hash mismatch for revision {}",
            revision.revision_id
        );
    }
    let expected = sha256_hex(&revision_material(
        &revision.parents,
        &revision.author,
        &revision.created_at_utc,
        &revision.message,
        &revision.snapshot_sha256,
    )?);
    if expected != revision.revision_id {
        bail!(
            "immutable revision hash mismatch for {}",
            revision.revision_id
        );
    }
    Ok(())
}

impl Repository {
    pub fn initialize(
        repository_id: Uuid,
        owner: impl Into<String>,
        snapshot: ModelSnapshot,
        created_at_utc: DateTime<Utc>,
    ) -> Result<Self> {
        let owner = owner.into();
        validate_name(&owner, "owner")?;
        let root = make_revision(
            vec![],
            owner.clone(),
            created_at_utc,
            "Initial model revision".to_owned(),
            snapshot,
        )?;
        let root_id = root.revision_id.clone();
        Ok(Self {
            schema_version: COLLABORATION_SCHEMA_VERSION.to_owned(),
            repository_id,
            revisions: BTreeMap::from([(root_id.clone(), root)]),
            branches: BTreeMap::from([(
                "main".to_owned(),
                Branch {
                    name: "main".to_owned(),
                    head: root_id,
                },
            )]),
            comments: BTreeMap::new(),
            access: AccessControl {
                principal_roles: BTreeMap::from([(owner, Role::Owner)]),
            },
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != COLLABORATION_SCHEMA_VERSION {
            bail!("unsupported collaboration schema '{}'", self.schema_version);
        }
        if self.branches.is_empty() || self.revisions.is_empty() {
            bail!("repository requires revisions and branches");
        }
        if self.access.principal_roles.is_empty() {
            bail!("repository requires at least one access-control entry");
        }
        if !self
            .access
            .principal_roles
            .values()
            .any(|role| *role == Role::Owner)
        {
            bail!("repository requires at least one owner");
        }
        for principal in self.access.principal_roles.keys() {
            validate_name(principal, "access-control principal")?;
        }
        for (id, revision) in &self.revisions {
            validate_revision(revision, Some(id))?;
            for parent in &revision.parents {
                if !self.revisions.contains_key(parent) {
                    bail!("revision {id} references missing parent {parent}");
                }
            }
        }
        for (name, branch) in &self.branches {
            validate_name(name, "branch name")?;
            if name != &branch.name || !self.revisions.contains_key(&branch.head) {
                bail!("invalid branch '{name}'");
            }
        }
        for (id, comment) in &self.comments {
            if id != &comment.comment_id {
                bail!("comment map key mismatch");
            }
            validate_name(&comment.author, "comment author")?;
            validate_name(&comment.body, "comment body")?;
            let revision = self
                .revisions
                .get(&comment.revision_id)
                .context("comment references missing revision")?;
            if let Some(object_id) = comment.object_id {
                if !revision.snapshot.objects.contains_key(&object_id) {
                    bail!("comment references missing object {object_id}");
                }
            }
        }
        Ok(())
    }

    pub fn head(&self, branch: &str) -> Result<&ModelRevision> {
        let branch = self
            .branches
            .get(branch)
            .with_context(|| format!("unknown branch '{branch}'"))?;
        self.revisions
            .get(&branch.head)
            .context("branch head revision is missing")
    }

    pub fn create_branch(
        &mut self,
        principal: &str,
        name: impl Into<String>,
        from_revision: &str,
    ) -> Result<()> {
        self.access.require(principal, Role::Editor)?;
        let name = name.into();
        validate_name(&name, "branch name")?;
        if self.branches.contains_key(&name) {
            bail!("branch '{name}' already exists");
        }
        if !self.revisions.contains_key(from_revision) {
            bail!("unknown source revision '{from_revision}'");
        }
        self.branches.insert(
            name.clone(),
            Branch {
                name,
                head: from_revision.to_owned(),
            },
        );
        Ok(())
    }

    pub fn commit(
        &mut self,
        principal: &str,
        branch: &str,
        expected_head: &str,
        changes: &[ObjectChange],
        message: impl Into<String>,
        created_at_utc: DateTime<Utc>,
    ) -> Result<RevisionId> {
        self.access.require(principal, Role::Editor)?;
        let current = self.head(branch)?;
        if current.revision_id != expected_head {
            bail!(
                "stale branch head: expected {expected_head}, actual {}",
                current.revision_id
            );
        }
        if changes.is_empty() {
            bail!("commit requires at least one object change");
        }
        let mut snapshot = current.snapshot.clone();
        for change in changes {
            match change {
                ObjectChange::Put { object } => {
                    object.validate()?;
                    snapshot.objects.insert(object.object_id, object.clone());
                }
                ObjectChange::Delete { object_id } => {
                    if snapshot.objects.remove(object_id).is_none() {
                        bail!("cannot delete missing object {object_id}");
                    }
                }
            }
        }
        let revision = make_revision(
            vec![current.revision_id.clone()],
            principal.to_owned(),
            created_at_utc,
            message.into(),
            snapshot,
        )?;
        let id = revision.revision_id.clone();
        self.revisions.insert(id.clone(), revision);
        self.branches
            .get_mut(branch)
            .with_context(|| format!("unknown branch '{branch}'"))?
            .head = id.clone();
        Ok(id)
    }

    pub fn add_comment(
        &mut self,
        principal: &str,
        revision_id: &str,
        object_id: Option<ObjectId>,
        body: impl Into<String>,
        created_at_utc: DateTime<Utc>,
    ) -> Result<Uuid> {
        self.access.require(principal, Role::Commenter)?;
        let revision = self
            .revisions
            .get(revision_id)
            .with_context(|| format!("unknown revision '{revision_id}'"))?;
        if let Some(id) = object_id {
            if !revision.snapshot.objects.contains_key(&id) {
                bail!("comment object {id} does not exist in revision");
            }
        }
        let body = body.into();
        validate_name(&body, "comment body")?;
        let id = Uuid::new_v4();
        self.comments.insert(
            id,
            ReviewComment {
                comment_id: id,
                revision_id: revision_id.to_owned(),
                object_id,
                author: principal.to_owned(),
                created_at_utc,
                body,
                state: ReviewState::Open,
            },
        );
        Ok(id)
    }

    pub fn set_review_state(
        &mut self,
        principal: &str,
        comment_id: Uuid,
        state: ReviewState,
    ) -> Result<()> {
        self.access.require(principal, Role::Maintainer)?;
        self.comments
            .get_mut(&comment_id)
            .with_context(|| format!("unknown comment {comment_id}"))?
            .state = state;
        Ok(())
    }

    pub fn grant(&mut self, principal: &str, subject: String, role: Role) -> Result<()> {
        self.access.require(principal, Role::Owner)?;
        validate_name(&subject, "subject")?;
        if subject == principal && role != Role::Owner {
            let owners = self
                .access
                .principal_roles
                .values()
                .filter(|value| **value == Role::Owner)
                .count();
            if owners == 1 {
                bail!("cannot demote the final repository owner");
            }
        }
        self.access.principal_roles.insert(subject, role);
        Ok(())
    }

    fn ancestors(&self, start: &str) -> Result<BTreeSet<RevisionId>> {
        let mut seen = BTreeSet::new();
        let mut pending = vec![start.to_owned()];
        while let Some(id) = pending.pop() {
            if seen.insert(id.clone()) {
                let revision = self
                    .revisions
                    .get(&id)
                    .with_context(|| format!("missing revision {id}"))?;
                pending.extend(revision.parents.iter().cloned());
            }
        }
        Ok(seen)
    }

    pub fn merge_base(&self, left: &str, right: &str) -> Result<RevisionId> {
        let left_ancestors = self.ancestors(left)?;
        let right_ancestors = self.ancestors(right)?;
        // Revision IDs are hashes, not chronology. Choose the common ancestor
        // nearest to `left` by breadth-first traversal.
        let common: BTreeSet<_> = left_ancestors
            .intersection(&right_ancestors)
            .cloned()
            .collect();
        let mut pending = VecDeque::new();
        pending.push_back(left.to_owned());
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if common.contains(&id) {
                return Ok(id);
            }
            let revision = self
                .revisions
                .get(&id)
                .with_context(|| format!("missing revision {id}"))?;
            pending.extend(revision.parents.iter().cloned());
        }
        bail!("branches have no common ancestor")
    }

    pub fn plan_merge(&self, target_branch: &str, source_branch: &str) -> Result<MergePlan> {
        let target = self.head(target_branch)?;
        let source = self.head(source_branch)?;
        let base_id = self.merge_base(&target.revision_id, &source.revision_id)?;
        let base = &self.revisions[&base_id].snapshot;
        let mut ids = BTreeSet::new();
        ids.extend(base.objects.keys().copied());
        ids.extend(target.snapshot.objects.keys().copied());
        ids.extend(source.snapshot.objects.keys().copied());

        let mut merged = target.snapshot.clone();
        let mut conflicts = Vec::new();
        for id in ids {
            let b = base.objects.get(&id);
            let t = target.snapshot.objects.get(&id);
            let s = source.snapshot.objects.get(&id);
            if t == s || s == b {
                continue;
            }
            if t == b {
                match s {
                    Some(value) => { merged.objects.insert(id, value.clone()); }
                    None => { merged.objects.remove(&id); }
                }
                continue;
            }
            conflicts.push(MergeConflict {
                object_id: id,
                base: b.cloned(),
                target: t.cloned(),
                source: s.cloned(),
            });
        }
        Ok(MergePlan {
            base_revision: base_id,
            target_revision: target.revision_id.clone(),
            source_revision: source.revision_id.clone(),
            merged_snapshot: merged,
            conflicts,
        })
    }

    pub fn merge(
        &mut self,
        principal: &str,
        target_branch: &str,
        source_branch: &str,
        resolutions: &BTreeMap<ObjectId, ConflictResolution>,
        message: impl Into<String>,
        created_at_utc: DateTime<Utc>,
    ) -> Result<RevisionId> {
        self.access.require(principal, Role::Maintainer)?;
        let plan = self.plan_merge(target_branch, source_branch)?;
        let mut snapshot = plan.merged_snapshot;
        for conflict in &plan.conflicts {
            let resolution = resolutions
                .get(&conflict.object_id)
                .with_context(|| format!("unresolved conflict {}", conflict.object_id))?;
            let selected = match resolution {
                ConflictResolution::UseTarget => conflict.target.clone(),
                ConflictResolution::UseSource => conflict.source.clone(),
                ConflictResolution::Delete => None,
                ConflictResolution::Put { object } => {
                    if object.object_id != conflict.object_id {
                        bail!("custom resolution object ID mismatch");
                    }
                    Some(object.clone())
                }
            };
            match selected {
                Some(object) => { snapshot.objects.insert(conflict.object_id, object); }
                None => { snapshot.objects.remove(&conflict.object_id); }
            }
        }
        for id in resolutions.keys() {
            if !plan.conflicts.iter().any(|item| &item.object_id == id) {
                bail!("resolution supplied for non-conflicting object {id}");
            }
        }
        let revision = make_revision(
            vec![plan.target_revision, plan.source_revision],
            principal.to_owned(),
            created_at_utc,
            message.into(),
            snapshot,
        )?;
        let id = revision.revision_id.clone();
        self.revisions.insert(id.clone(), revision);
        self.branches
            .get_mut(target_branch)
            .with_context(|| format!("unknown branch '{target_branch}'"))?
            .head = id.clone();
        Ok(id)
    }

    pub fn export_sync_bundle(
        &self,
        principal: &str,
        known_revisions: &BTreeSet<RevisionId>,
    ) -> Result<SyncBundle> {
        self.access.require(principal, Role::Viewer)?;
        let revisions = self
            .revisions
            .iter()
            .filter(|(id, _)| !known_revisions.contains(*id))
            .map(|(id, revision)| (id.clone(), revision.clone()))
            .collect();
        let mut bundle = SyncBundle {
            schema_version: SYNC_SCHEMA_VERSION.to_owned(),
            repository_id: self.repository_id,
            revisions,
            branch_heads: self
                .branches
                .iter()
                .map(|(name, branch)| (name.clone(), branch.head.clone()))
                .collect(),
            comments: self.comments.clone(),
            bundle_sha256: String::new(),
        };
        bundle.bundle_sha256 = bundle.calculate_sha256()?;
        Ok(bundle)
    }

    pub fn import_sync_bundle(
        &mut self,
        principal: &str,
        bundle: SyncBundle,
    ) -> Result<SyncReport> {
        self.access.require(principal, Role::Editor)?;
        bundle.validate()?;
        if bundle.repository_id != self.repository_id {
            bail!("sync bundle belongs to another repository");
        }

        // Apply remotely supplied data transactionally. A malformed revision,
        // branch, or comment must not leave a partially updated repository.
        let mut staged = self.clone();
        let report = staged.apply_sync_bundle(bundle)?;
        staged.validate()?;
        *self = staged;
        Ok(report)
    }

    fn apply_sync_bundle(&mut self, bundle: SyncBundle) -> Result<SyncReport> {
        let SyncBundle {
            revisions,
            branch_heads,
            comments,
            ..
        } = bundle;
        let mut pending = revisions;
        let mut imported = Vec::new();

        loop {
            let ready: Vec<_> = pending
                .iter()
                .filter(|(_, revision)| {
                    revision
                        .parents
                        .iter()
                        .all(|parent| self.revisions.contains_key(parent))
                })
                .map(|(id, _)| id.clone())
                .collect();
            if ready.is_empty() {
                break;
            }

            for id in ready {
                let revision = pending
                    .remove(&id)
                    .with_context(|| format!("pending revision {id} disappeared"))?;
                validate_revision(&revision, Some(&id))?;
                if let Some(existing) = self.revisions.get(&id) {
                    if existing != &revision {
                        bail!("revision {id} conflicts with an existing revision");
                    }
                } else {
                    self.revisions.insert(id.clone(), revision);
                    imported.push(id);
                }
            }
        }
        if !pending.is_empty() {
            bail!("sync bundle contains revisions with unavailable parents");
        }

        let mut branch_conflicts = Vec::new();
        for (name, remote_head) in branch_heads {
            validate_name(&name, "remote branch name")?;
            if !self.revisions.contains_key(&remote_head) {
                bail!("remote branch '{name}' points to unavailable revision");
            }
            let local_head = self.branches.get(&name).map(|branch| branch.head.clone());
            match local_head {
                None => {
                    self.branches.insert(
                        name.clone(),
                        Branch {
                            name,
                            head: remote_head,
                        },
                    );
                }
                Some(local_head) if local_head == remote_head => {}
                Some(local_head) => {
                    let remote_ancestors = self.ancestors(&remote_head)?;
                    let local_ancestors = self.ancestors(&local_head)?;
                    if remote_ancestors.contains(&local_head) {
                        self.branches
                            .get_mut(&name)
                            .with_context(|| format!("unknown branch '{name}'"))?
                            .head = remote_head;
                    } else if !local_ancestors.contains(&remote_head) {
                        branch_conflicts.push(BranchConflict {
                            branch: name,
                            local_head,
                            remote_head,
                        });
                    }
                }
            }
        }

        for (id, comment) in comments {
            if id != comment.comment_id {
                bail!("synced comment map key mismatch");
            }
            validate_name(&comment.author, "synced comment author")?;
            validate_name(&comment.body, "synced comment body")?;
            let revision = self
                .revisions
                .get(&comment.revision_id)
                .context("synced comment references missing revision")?;
            if let Some(object_id) = comment.object_id {
                if !revision.snapshot.objects.contains_key(&object_id) {
                    bail!("synced comment references missing object {object_id}");
                }
            }
            if let Some(existing) = self.comments.get(&id) {
                if existing != &comment {
                    bail!("comment {id} conflicts with an existing comment");
                }
            } else {
                self.comments.insert(id, comment);
            }
        }

        Ok(SyncReport {
            imported_revisions: imported,
            branch_conflicts,
        })
    }

}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeConflict {
    pub object_id: ObjectId,
    pub base: Option<DomainObject>,
    pub target: Option<DomainObject>,
    pub source: Option<DomainObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergePlan {
    pub base_revision: RevisionId,
    pub target_revision: RevisionId,
    pub source_revision: RevisionId,
    pub merged_snapshot: ModelSnapshot,
    pub conflicts: Vec<MergeConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "resolution", rename_all = "snake_case")]
pub enum ConflictResolution {
    UseTarget,
    UseSource,
    Delete,
    Put { object: DomainObject },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncBundle {
    pub schema_version: String,
    pub repository_id: Uuid,
    pub revisions: BTreeMap<RevisionId, ModelRevision>,
    pub branch_heads: BTreeMap<String, RevisionId>,
    pub comments: BTreeMap<Uuid, ReviewComment>,
    pub bundle_sha256: String,
}

impl SyncBundle {
    fn calculate_sha256(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.bundle_sha256.clear();
        Ok(sha256_hex(&canonical_json_bytes(&copy)?))
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SYNC_SCHEMA_VERSION {
            bail!("unsupported sync schema '{}'", self.schema_version);
        }
        if self.calculate_sha256()? != self.bundle_sha256 {
            bail!("sync bundle hash mismatch");
        }
        for (id, revision) in &self.revisions {
            validate_revision(revision, Some(id))?;
        }
        for name in self.branch_heads.keys() {
            validate_name(name, "sync branch name")?;
        }
        for (id, comment) in &self.comments {
            if id != &comment.comment_id {
                bail!("sync comment map key mismatch");
            }
            validate_name(&comment.author, "sync comment author")?;
            validate_name(&comment.body, "sync comment body")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchConflict {
    pub branch: String,
    pub local_head: RevisionId,
    pub remote_head: RevisionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncReport {
    pub imported_revisions: Vec<RevisionId>,
    pub branch_conflicts: Vec<BranchConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptedBundle {
    pub schema_version: String,
    pub algorithm: String,
    pub repository_id: Uuid,
    pub nonce_base64: String,
    pub ciphertext_base64: String,
    pub plaintext_sha256: String,
}

impl EncryptedBundle {
    pub fn encrypt(bundle: &SyncBundle, key_bytes: &[u8; 32]) -> Result<Self> {
        let mut nonce = [0_u8; 24];
        getrandom::getrandom(&mut nonce)
            .map_err(|error| anyhow::anyhow!("cannot obtain encryption nonce: {error}"))?;
        Self::encrypt_with_nonce(bundle, key_bytes, nonce)
    }

    pub fn encrypt_with_nonce(
        bundle: &SyncBundle,
        key_bytes: &[u8; 32],
        nonce: [u8; 24],
    ) -> Result<Self> {
        bundle.validate()?;
        let plaintext = canonical_json_bytes(bundle)?;
        let aad = canonical_json_bytes(&serde_json::json!({
            "schema_version": ENCRYPTED_BUNDLE_SCHEMA_VERSION,
            "algorithm": "XChaCha20-Poly1305",
            "repository_id": bundle.repository_id
        }))?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key_bytes));
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload { msg: &plaintext, aad: &aad },
            )
            .map_err(|_| anyhow::anyhow!("bundle encryption failed"))?;
        Ok(Self {
            schema_version: ENCRYPTED_BUNDLE_SCHEMA_VERSION.to_owned(),
            algorithm: "XChaCha20-Poly1305".to_owned(),
            repository_id: bundle.repository_id,
            nonce_base64: STANDARD.encode(nonce),
            ciphertext_base64: STANDARD.encode(ciphertext),
            plaintext_sha256: sha256_hex(&plaintext),
        })
    }

    pub fn decrypt(&self, key_bytes: &[u8; 32]) -> Result<SyncBundle> {
        if self.schema_version != ENCRYPTED_BUNDLE_SCHEMA_VERSION
            || self.algorithm != "XChaCha20-Poly1305"
        {
            bail!("unsupported encrypted bundle format");
        }
        let nonce = STANDARD.decode(&self.nonce_base64).context("invalid nonce base64")?;
        if nonce.len() != 24 {
            bail!("XChaCha20 nonce must be 24 bytes");
        }
        let ciphertext = STANDARD
            .decode(&self.ciphertext_base64)
            .context("invalid ciphertext base64")?;
        let aad = canonical_json_bytes(&serde_json::json!({
            "schema_version": self.schema_version,
            "algorithm": self.algorithm,
            "repository_id": self.repository_id
        }))?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key_bytes));
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload { msg: &ciphertext, aad: &aad },
            )
            .map_err(|_| anyhow::anyhow!("bundle authentication or decryption failed"))?;
        if sha256_hex(&plaintext) != self.plaintext_sha256 {
            bail!("decrypted plaintext hash mismatch");
        }
        let bundle: SyncBundle =
            serde_json::from_slice(&plaintext).context("decrypted bundle is invalid JSON")?;
        bundle.validate()?;
        if bundle.repository_id != self.repository_id {
            bail!("encrypted bundle repository mismatch");
        }
        Ok(bundle)
    }
}
