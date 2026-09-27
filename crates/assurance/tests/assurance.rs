use chrono::{TimeZone, Utc};
use std::{collections::BTreeMap, fs};
use structural_assurance::*;

fn artifact(path: &str, bytes: &[u8]) -> EvidenceArtifact {
    EvidenceArtifact {
        path: path.to_owned(),
        media_type: "application/json".to_owned(),
        sha256: structural_audit::sha256_hex(bytes),
        byte_length: bytes.len() as u64,
    }
}

fn catalog(input: EvidenceArtifact, result: EvidenceArtifact) -> AssuranceCatalog {
    AssuranceCatalog {
        schema_version: ASSURANCE_SCHEMA_VERSION.to_owned(),
        product_name: "Test".to_owned(),
        product_version: "1".to_owned(),
        published_at_utc: Utc.with_ymd_and_hms(2026, 9, 27, 0, 0, 0).unwrap(),
        scope_statement: "Reference only".to_owned(),
        benchmarks: vec![BenchmarkCase {
            benchmark_id: "static.sdof".to_owned(),
            title: "SDoF".to_owned(),
            analysis_type: "linear_static".to_owned(),
            description: "P/k".to_owned(),
            inputs: vec![input],
            reference_artifacts: vec![],
            expected_metrics: vec![ExpectedMetric {
                metric_id: "displacement_m".to_owned(),
                description: "Displacement".to_owned(),
                unit: "m".to_owned(),
                minimum: 0.009999,
                maximum: 0.010001,
            }],
            reference_sources: vec!["Analytical P/k".to_owned()],
            limitation_ids: vec!["reference-only".to_owned()],
        }],
        validations: vec![SolverValidation {
            validation_id: "run-1".to_owned(),
            benchmark_id: "static.sdof".to_owned(),
            solver_id: "reference".to_owned(),
            solver_version: "1".to_owned(),
            platform: "test".to_owned(),
            executed_at_utc: Some(Utc.with_ymd_and_hms(2026, 9, 27, 0, 0, 0).unwrap()),
            status: ValidationStatus::Passed,
            observed_metrics: BTreeMap::from([("displacement_m".to_owned(), 0.01)]),
            result: Some(result),
            notes: vec![],
        }],
        known_limitations: vec![KnownLimitation {
            limitation_id: "reference-only".to_owned(),
            title: "Reference".to_owned(),
            description: "Not certified".to_owned(),
            affected_components: vec!["solver".to_owned()],
            severity: "high".to_owned(),
            workaround: None,
            open: true,
        }],
        traceability: vec![TraceabilityEntry {
            requirement_id: "REQ-1".to_owned(),
            source: "plan".to_owned(),
            claim: "static equilibrium".to_owned(),
            benchmark_ids: vec!["static.sdof".to_owned()],
            evidence_paths: vec!["input.json".to_owned()],
        }],
        independent_lab_results: vec![],
    }
}

#[test]
fn builds_hash_verified_portal_and_detects_tampering() {
    let base = std::env::temp_dir().join(format!("structural-assurance-{}", std::process::id()));
    let source = base.join("source");
    let output = base.join("portal");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&source).unwrap();
    let input = br#"{"force_n":1000,"stiffness_n_per_m":100000}"#;
    let result = br#"{"displacement_m":0.01}"#;
    fs::write(source.join("input.json"), input).unwrap();
    fs::write(source.join("result.json"), result).unwrap();
    let catalog = catalog(artifact("input.json", input), artifact("result.json", result));
    let manifest = build_portal(&catalog, &source, &output, Utc::now()).unwrap();
    assert_eq!(manifest.artifacts.len(), 2);
    assert!(output.join("index.html").exists());

    fs::write(source.join("result.json"), b"tampered").unwrap();
    assert!(build_portal(&catalog, &source, &output, Utc::now()).is_err());
    let _ = fs::remove_dir_all(base);
}

#[test]
fn declared_pass_must_match_expected_range() {
    let input = artifact("input.json", b"{}");
    let result = artifact("result.json", b"{}");
    let mut value = catalog(input, result);
    value.validations[0].observed_metrics.insert("displacement_m".to_owned(), 2.0);
    assert!(value.validate().is_err());
}
