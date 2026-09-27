# Backup and restore runbook

Back up immutable revisions, artifact blobs, metadata, key references and audit events.
Never export HSM private keys as application backup data. Quarterly, restore into an
isolated environment, verify all content digests and access controls, record actual RPO/RTO,
and test rollback. A backup is not accepted until restore has been demonstrated.
