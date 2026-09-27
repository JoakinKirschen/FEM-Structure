use structural_solver_api::{
    NonlinearAnalysisInput, NonlinearExecutionOptions, NonlinearStructuralSolver,
};
use structural_solver_nonlinear::ReferenceNonlinearSpringSolver;

fn fixture(name: &str) -> NonlinearAnalysisInput {
    serde_json::from_str(match name {
        "linear" => include_str!("fixtures/linear-elastic.json"),
        "material" => include_str!("fixtures/bilinear-material.json"),
        "geometric" => include_str!("fixtures/cubic-geometric.json"),
        _ => panic!("unknown fixture"),
    })
    .unwrap()
}

#[test]
fn validated_benchmark_suite() {
    let cases = [
        ("linear", 0.1_f64, 1.0e-10_f64),
        ("material", 0.11_f64, 1.0e-9_f64),
        ("geometric", 0.1_f64, 1.0e-9_f64),
    ];
    for (name, expected, tolerance) in cases {
        let result = ReferenceNonlinearSpringSolver
            .solve_nonlinear(
                &fixture(name),
                NonlinearExecutionOptions::default(),
                None,
            )
            .unwrap();
        assert!(result.converged(), "{name}: {:?}", result.termination);
        let actual = result.final_states[0].displacement_m;
        assert!(
            (actual - expected).abs() <= tolerance,
            "{name}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn deterministic_profile_repeats_exactly() {
    let input = fixture("geometric");
    let options = NonlinearExecutionOptions::default();
    let first = ReferenceNonlinearSpringSolver
        .solve_nonlinear(&input, options, None)
        .unwrap();
    let second = ReferenceNonlinearSpringSolver
        .solve_nonlinear(&input, options, None)
        .unwrap();
    assert_eq!(first, second);
}
