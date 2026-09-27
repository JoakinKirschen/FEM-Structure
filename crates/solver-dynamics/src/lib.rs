//! Deterministic dense reference solvers for modal, linearized buckling and
//! direct-integration time-history analysis.
//!
//! The dense implementation is intentionally small and auditable. It establishes
//! contracts and benchmarks; production-scale sparse eigensolvers remain pluggable.

use anyhow::{bail, Context, Result};
use std::f64::consts::PI;
use structural_solver_api::{
    BucklingAnalysisInput, BucklingAnalysisResult, BucklingMode, DampingModel,
    DynamicStructuralSolver, EigenExecutionOptions, EigenMode, LinearDynamicSystem,
    ModalAnalysisResult, ModeNormalization, SolverToleranceDeclaration,
    TimeHistoryAnalysisInput, TimeHistoryAnalysisResult, TimeHistoryExecutionOptions,
    TimeHistoryStepResult, DYNAMICS_SCHEMA_VERSION,
};

pub struct ReferenceDynamicSolver;

type Matrix = Vec<Vec<f64>>;

fn zeros(n: usize) -> Matrix {
    vec![vec![0.0; n]; n]
}

fn identity(n: usize) -> Matrix {
    let mut result = zeros(n);
    for (i, row) in result.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    result
}

fn transpose(a: &Matrix) -> Matrix {
    let n = a.len();
    let mut out = zeros(n);
    for i in 0..n {
        for j in 0..n {
            out[j][i] = a[i][j];
        }
    }
    out
}

fn mat_mul(a: &Matrix, b: &Matrix) -> Matrix {
    let n = a.len();
    let mut out = zeros(n);
    for i in 0..n {
        for k in 0..n {
            for j in 0..n {
                out[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    out
}

fn mat_vec(a: &Matrix, x: &[f64]) -> Vec<f64> {
    a.iter()
        .map(|row| row.iter().zip(x).map(|(v, x)| v * x).sum())
        .collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

fn cholesky(a: &Matrix, tolerance: f64, name: &str) -> Result<Matrix> {
    let n = a.len();
    let mut l = zeros(n);
    for i in 0..n {
        for j in 0..=i {
            let mut value = a[i][j];
            for k in 0..j {
                value -= l[i][k] * l[j][k];
            }
            if i == j {
                if !value.is_finite() || value <= tolerance {
                    bail!("{} matrix is not positive definite at pivot {}", name, i);
                }
                l[i][j] = value.sqrt();
            } else {
                l[i][j] = value / l[j][j];
            }
        }
    }
    Ok(l)
}

fn inverse_lower(l: &Matrix) -> Matrix {
    let n = l.len();
    let mut inv = zeros(n);
    for column in 0..n {
        for i in 0..n {
            let rhs = if i == column { 1.0 } else { 0.0 };
            let correction: f64 = (0..i).map(|j| l[i][j] * inv[j][column]).sum();
            inv[i][column] = (rhs - correction) / l[i][i];
        }
    }
    inv
}

fn jacobi_eigen(
    input: &Matrix,
    tolerance: f64,
    maximum_sweeps: u32,
) -> Result<(Vec<f64>, Matrix)> {
    let n = input.len();
    let mut a = input.clone();
    let mut vectors = identity(n);

    for _ in 0..maximum_sweeps {
        let mut p = 0;
        let mut q = 0;
        let mut largest = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                if a[i][j].abs() > largest {
                    largest = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        let diagonal_scale = (0..n).map(|i| a[i][i].abs()).fold(1.0_f64, f64::max);
        if largest <= tolerance * diagonal_scale {
            let values = (0..n).map(|i| a[i][i]).collect();
            return Ok((values, vectors));
        }

        let angle = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
        let c = angle.cos();
        let s = angle.sin();

        for i in 0..n {
            if i != p && i != q {
                let aip = a[i][p];
                let aiq = a[i][q];
                a[i][p] = c * aip - s * aiq;
                a[p][i] = a[i][p];
                a[i][q] = s * aip + c * aiq;
                a[q][i] = a[i][q];
            }
        }
        let app = a[p][p];
        let aqq = a[q][q];
        let apq = a[p][q];
        a[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        a[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;

        for row in &mut vectors {
            let vip = row[p];
            let viq = row[q];
            row[p] = c * vip - s * viq;
            row[q] = s * vip + c * viq;
        }
    }
    bail!("Jacobi eigenvalue iteration exceeded the maximum sweep count")
}

fn generalized_eigen(
    numerator: &Matrix,
    denominator: &Matrix,
    options: EigenExecutionOptions,
    denominator_name: &str,
) -> Result<Vec<(f64, Vec<f64>)>> {
    let l = cholesky(
        denominator,
        options.positive_definite_tolerance,
        denominator_name,
    )?;
    let l_inv = inverse_lower(&l);
    let transformed = mat_mul(&mat_mul(&l_inv, numerator), &transpose(&l_inv));
    let (values, y_vectors) =
        jacobi_eigen(&transformed, options.jacobi_tolerance, options.maximum_sweeps)?;

    let l_inv_t = transpose(&l_inv);
    let mut pairs: Vec<_> = values
        .into_iter()
        .enumerate()
        .map(|(column, value)| {
            let y: Vec<f64> = (0..y_vectors.len())
                .map(|row| y_vectors[row][column])
                .collect();
            (value, mat_vec(&l_inv_t, &y))
        })
        .collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(pairs)
}

fn normalize_mode(
    vector: &mut [f64],
    denominator: &Matrix,
    normalization: ModeNormalization,
) -> Result<()> {
    let scale = match normalization {
        ModeNormalization::Mass => dot(vector, &mat_vec(denominator, vector)).sqrt(),
        ModeNormalization::MaximumAbsoluteComponent => {
            vector.iter().map(|v| v.abs()).fold(0.0_f64, f64::max)
        }
    };
    if !scale.is_finite() || scale <= 0.0 {
        bail!("cannot normalize a zero or non-finite eigenvector");
    }
    for value in vector {
        *value /= scale;
    }
    Ok(())
}

fn eigen_residual(
    numerator: &Matrix,
    denominator: &Matrix,
    eigenvalue: f64,
    vector: &[f64],
) -> f64 {
    let left = mat_vec(numerator, vector);
    let right = mat_vec(denominator, vector);
    let residual: Vec<f64> = left
        .iter()
        .zip(right)
        .map(|(a, b)| a - eigenvalue * b)
        .collect();
    norm(&residual) / norm(&left).max((eigenvalue.abs() * norm(vector)).max(1.0))
}

fn tolerance_declaration(options: EigenExecutionOptions) -> SolverToleranceDeclaration {
    SolverToleranceDeclaration {
        symmetry_tolerance: options.symmetry_tolerance,
        positive_definite_tolerance: options.positive_definite_tolerance,
        iteration_tolerance: options.jacobi_tolerance,
        residual_tolerance: options.residual_tolerance,
        maximum_iterations: options.maximum_sweeps,
        deterministic_profile: options.deterministic_profile,
    }
}

fn damping_matrix(input: &TimeHistoryAnalysisInput) -> Matrix {
    let n = input.system.dof_ids.len();
    let mut c = zeros(n);
    match &input.damping {
        DampingModel::None => {}
        DampingModel::Rayleigh {
            mass_coefficient_per_s,
            stiffness_coefficient_s,
        } => {
            for i in 0..n {
                for j in 0..n {
                    c[i][j] = *mass_coefficient_per_s * input.system.mass_matrix_kg[i][j]
                        + *stiffness_coefficient_s * input.system.stiffness_matrix_n_per_m[i][j];
                }
            }
        }
        DampingModel::ReferenceCriticalRatio {
            critical_ratio,
            reference_frequency_hz,
        } => {
            let coefficient = 2.0 * *critical_ratio * 2.0 * PI * *reference_frequency_hz;
            for i in 0..n {
                for j in 0..n {
                    c[i][j] = coefficient * input.system.mass_matrix_kg[i][j];
                }
            }
        }
    }
    c
}

fn solve_dense(a: &Matrix, b: &[f64], pivot_tolerance: f64) -> Result<Vec<f64>> {
    let n = a.len();
    let mut augmented: Vec<Vec<f64>> = a
        .iter()
        .zip(b)
        .map(|(row, rhs)| {
            let mut out = row.clone();
            out.push(*rhs);
            out
        })
        .collect();

    for column in 0..n {
        let pivot = (column..n)
            .max_by(|&i, &j| augmented[i][column].abs().total_cmp(&augmented[j][column].abs()))
            .unwrap();
        if augmented[pivot][column].abs() <= pivot_tolerance {
            bail!("effective matrix is singular at pivot {}", column);
        }
        augmented.swap(column, pivot);
        let pivot_value = augmented[column][column];
        for value in &mut augmented[column][column..=n] {
            *value /= pivot_value;
        }
        let pivot_row = augmented[column].clone();
        for (row_index, row) in augmented.iter_mut().enumerate() {
            if row_index == column {
                continue;
            }
            let factor = row[column];
            for j in column..=n {
                row[j] -= factor * pivot_row[j];
            }
        }
    }
    Ok(augmented.into_iter().map(|row| row[n]).collect())
}

impl DynamicStructuralSolver for ReferenceDynamicSolver {
    fn id(&self) -> &'static str {
        "sweco.structural.dense-dynamics-reference"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn solve_modal(
        &self,
        system: &LinearDynamicSystem,
        options: EigenExecutionOptions,
    ) -> Result<ModalAnalysisResult> {
        options.validate()?;
        system.validate(options.symmetry_tolerance)?;
        let pairs = generalized_eigen(
            &system.stiffness_matrix_n_per_m,
            &system.mass_matrix_kg,
            options,
            "mass",
        )?;
        let mut modes = Vec::new();
        for (eigenvalue, mut vector) in pairs
            .into_iter()
            .filter(|(value, _)| *value > options.positive_definite_tolerance)
            .take(options.requested_modes)
        {
            normalize_mode(&mut vector, &system.mass_matrix_kg, options.normalization)?;
            let residual = eigen_residual(
                &system.stiffness_matrix_n_per_m,
                &system.mass_matrix_kg,
                eigenvalue,
                &vector,
            );
            if residual > options.residual_tolerance {
                bail!("modal eigenpair residual {} exceeds tolerance", residual);
            }
            let omega = eigenvalue.sqrt();
            modes.push(EigenMode {
                mode_number: modes.len() + 1,
                eigenvalue,
                circular_frequency_rad_per_s: Some(omega),
                frequency_hz: Some(omega / (2.0 * PI)),
                generalized_mass: dot(&vector, &mat_vec(&system.mass_matrix_kg, &vector)),
                generalized_stiffness: dot(
                    &vector,
                    &mat_vec(&system.stiffness_matrix_n_per_m, &vector),
                ),
                residual_norm: residual,
                vector,
            });
        }
        if modes.is_empty() {
            bail!("no positive modal eigenvalues were found");
        }
        Ok(ModalAnalysisResult {
            schema_version: DYNAMICS_SCHEMA_VERSION.to_owned(),
            analysis_id: system.analysis_id,
            solver_id: self.id().to_owned(),
            solver_version: self.version().to_owned(),
            normalization: options.normalization,
            tolerances: tolerance_declaration(options),
            modes,
            warnings: vec![
                "Dense reference eigensolver; production sparse/subspace solvers are not included."
                    .to_owned(),
                "Rigid-body and non-positive modes are omitted from the returned modal set."
                    .to_owned(),
            ],
        })
    }

    fn solve_buckling(
        &self,
        input: &BucklingAnalysisInput,
        options: EigenExecutionOptions,
    ) -> Result<BucklingAnalysisResult> {
        options.validate()?;
        input.validate(options.symmetry_tolerance)?;
        let pairs = generalized_eigen(
            &input.system.stiffness_matrix_n_per_m,
            &input.geometric_stiffness_n_per_m,
            options,
            "geometric stiffness",
        )?;
        let mut modes = Vec::new();
        for (load_factor, mut vector) in pairs
            .into_iter()
            .filter(|(value, _)| *value > options.positive_definite_tolerance)
            .take(options.requested_modes)
        {
            normalize_mode(
                &mut vector,
                &input.geometric_stiffness_n_per_m,
                options.normalization,
            )?;
            let residual = eigen_residual(
                &input.system.stiffness_matrix_n_per_m,
                &input.geometric_stiffness_n_per_m,
                load_factor,
                &vector,
            );
            if residual > options.residual_tolerance {
                bail!("buckling eigenpair residual {} exceeds tolerance", residual);
            }
            modes.push(BucklingMode {
                mode_number: modes.len() + 1,
                load_factor,
                elastic_energy_measure: dot(
                    &vector,
                    &mat_vec(&input.system.stiffness_matrix_n_per_m, &vector),
                ),
                geometric_energy_measure: dot(
                    &vector,
                    &mat_vec(&input.geometric_stiffness_n_per_m, &vector),
                ),
                residual_norm: residual,
                vector,
            });
        }
        if modes.is_empty() {
            bail!("no positive buckling factors were found");
        }
        Ok(BucklingAnalysisResult {
            schema_version: DYNAMICS_SCHEMA_VERSION.to_owned(),
            analysis_id: input.system.analysis_id,
            solver_id: self.id().to_owned(),
            solver_version: self.version().to_owned(),
            normalization: options.normalization,
            tolerances: tolerance_declaration(options),
            modes,
            warnings: vec![
                concat!(
                    "Linearized reference buckling only; the supplied geometric matrix must ",
                    "be symmetric positive definite in this pass."
                )
                .to_owned(),
                "Imperfections, nonlinear post-buckling response and mode interaction are excluded."
                    .to_owned(),
            ],
        })
    }

    fn solve_time_history(
        &self,
        input: &TimeHistoryAnalysisInput,
        options: TimeHistoryExecutionOptions,
    ) -> Result<TimeHistoryAnalysisResult> {
        options.validate()?;
        input.validate(options.matrix_symmetry_tolerance)?;
        let n = input.system.dof_ids.len();
        let m = &input.system.mass_matrix_kg;
        let k = &input.system.stiffness_matrix_n_per_m;
        let c = damping_matrix(input);
        let beta = options.newmark_beta;
        let gamma = options.newmark_gamma;

        let mut displacement = input.initial_displacement_m.clone();
        let mut velocity = input.initial_velocity_m_per_s.clone();
        let initial_rhs: Vec<f64> = {
            let cv = mat_vec(&c, &velocity);
            let ku = mat_vec(k, &displacement);
            input.loads[0]
                .force_n
                .iter()
                .zip(cv.iter().zip(ku))
                .map(|(force, (damping, stiffness))| force - damping - stiffness)
                .collect()
        };
        let mut acceleration = solve_dense(m, &initial_rhs, options.pivot_tolerance)
            .context("cannot compute initial acceleration")?;

        let initial_residual = equilibrium_residual(m, &acceleration, &c, &velocity, k, &displacement, &input.loads[0].force_n);
        let mut steps = vec![TimeHistoryStepResult {
            step: 0,
            time_s: input.loads[0].time_s,
            displacement_m: displacement.clone(),
            velocity_m_per_s: velocity.clone(),
            acceleration_m_per_s2: acceleration.clone(),
            applied_force_n: input.loads[0].force_n.clone(),
            equilibrium_residual_norm_n: initial_residual,
        }];

        for index in 1..input.loads.len() {
            let dt = input.loads[index].time_s - input.loads[index - 1].time_s;
            let a0 = 1.0 / (beta * dt * dt);
            let a1 = gamma / (beta * dt);
            let a2 = 1.0 / (beta * dt);
            let a3 = 1.0 / (2.0 * beta) - 1.0;
            let a4 = gamma / beta - 1.0;
            let a5 = dt * (gamma / (2.0 * beta) - 1.0);

            let mut effective = zeros(n);
            for i in 0..n {
                for j in 0..n {
                    effective[i][j] = k[i][j] + a0 * m[i][j] + a1 * c[i][j];
                }
            }
            let mass_predictor: Vec<f64> = (0..n)
                .map(|i| a0 * displacement[i] + a2 * velocity[i] + a3 * acceleration[i])
                .collect();
            let damping_predictor: Vec<f64> = (0..n)
                .map(|i| a1 * displacement[i] + a4 * velocity[i] + a5 * acceleration[i])
                .collect();
            let m_term = mat_vec(m, &mass_predictor);
            let c_term = mat_vec(&c, &damping_predictor);
            let rhs: Vec<f64> = input.loads[index]
                .force_n
                .iter()
                .zip(m_term.iter().zip(c_term))
                .map(|(force, (mass, damping))| force + mass + damping)
                .collect();
            let next_displacement = solve_dense(&effective, &rhs, options.pivot_tolerance)?;
            let next_acceleration: Vec<f64> = (0..n)
                .map(|i| {
                    a0 * (next_displacement[i] - displacement[i])
                        - a2 * velocity[i]
                        - a3 * acceleration[i]
                })
                .collect();
            let next_velocity: Vec<f64> = (0..n)
                .map(|i| {
                    velocity[i]
                        + dt * ((1.0 - gamma) * acceleration[i] + gamma * next_acceleration[i])
                })
                .collect();

            let residual = equilibrium_residual(
                m,
                &next_acceleration,
                &c,
                &next_velocity,
                k,
                &next_displacement,
                &input.loads[index].force_n,
            );
            let force_scale = norm(&input.loads[index].force_n).max(1.0);
            if residual > options.equilibrium_residual_tolerance * force_scale {
                bail!(
                    "time-history equilibrium residual {} exceeds tolerance at step {}",
                    residual,
                    index
                );
            }
            displacement = next_displacement;
            velocity = next_velocity;
            acceleration = next_acceleration;
            steps.push(TimeHistoryStepResult {
                step: index,
                time_s: input.loads[index].time_s,
                displacement_m: displacement.clone(),
                velocity_m_per_s: velocity.clone(),
                acceleration_m_per_s2: acceleration.clone(),
                applied_force_n: input.loads[index].force_n.clone(),
                equilibrium_residual_norm_n: residual,
            });
        }

        Ok(TimeHistoryAnalysisResult {
            schema_version: DYNAMICS_SCHEMA_VERSION.to_owned(),
            analysis_id: input.system.analysis_id,
            solver_id: self.id().to_owned(),
            solver_version: self.version().to_owned(),
            damping: input.damping.clone(),
            newmark_beta: beta,
            newmark_gamma: gamma,
            tolerances: SolverToleranceDeclaration {
                symmetry_tolerance: options.matrix_symmetry_tolerance,
                positive_definite_tolerance: options.pivot_tolerance,
                iteration_tolerance: 0.0,
                residual_tolerance: options.equilibrium_residual_tolerance,
                maximum_iterations: 1,
                deterministic_profile: options.deterministic_profile,
            },
            steps,
            warnings: vec![
                "Linear direct integration with prescribed load samples; no nonlinear events."
                    .to_owned(),
                "Loads are evaluated at provided sample times without interpolation or base-motion conversion."
                    .to_owned(),
            ],
        })
    }
}

fn equilibrium_residual(
    mass: &Matrix,
    acceleration: &[f64],
    damping: &Matrix,
    velocity: &[f64],
    stiffness: &Matrix,
    displacement: &[f64],
    force: &[f64],
) -> f64 {
    let ma = mat_vec(mass, acceleration);
    let cv = mat_vec(damping, velocity);
    let ku = mat_vec(stiffness, displacement);
    let residual: Vec<f64> = (0..force.len())
        .map(|i| force[i] - ma[i] - cv[i] - ku[i])
        .collect();
    norm(&residual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use structural_solver_api::{
        DampingModel, DynamicStructuralSolver, EigenExecutionOptions, LinearDynamicSystem,
        TimeHistoryAnalysisInput, TimeHistoryExecutionOptions, TimeHistoryLoadPoint,
        DYNAMICS_SCHEMA_VERSION,
    };
    use uuid::Uuid;

    fn diagonal_system() -> LinearDynamicSystem {
        LinearDynamicSystem {
            schema_version: DYNAMICS_SCHEMA_VERSION.to_owned(),
            analysis_id: Uuid::from_u128(1),
            dof_ids: vec![Uuid::from_u128(2), Uuid::from_u128(3)],
            mass_matrix_kg: vec![vec![2.0, 0.0], vec![0.0, 1.0]],
            stiffness_matrix_n_per_m: vec![vec![200.0, 0.0], vec![0.0, 400.0]],
        }
    }

    #[test]
    fn extracts_and_mass_normalizes_modal_frequencies() {
        let result = ReferenceDynamicSolver
            .solve_modal(&diagonal_system(), EigenExecutionOptions::default())
            .unwrap();
        assert_eq!(result.modes.len(), 2);
        assert!((result.modes[0].eigenvalue - 100.0).abs() < 1.0e-10);
        assert!((result.modes[1].eigenvalue - 400.0).abs() < 1.0e-10);
        assert!((result.modes[0].generalized_mass - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn extracts_reference_buckling_factors() {
        let input = BucklingAnalysisInput {
            system: diagonal_system(),
            geometric_stiffness_n_per_m: vec![vec![20.0, 0.0], vec![0.0, 100.0]],
        };
        let result = ReferenceDynamicSolver
            .solve_buckling(&input, EigenExecutionOptions::default())
            .unwrap();
        assert!((result.modes[0].load_factor - 4.0).abs() < 1.0e-12);
        assert!((result.modes[1].load_factor - 10.0).abs() < 1.0e-12);
    }

    #[test]
    fn integrates_sdof_constant_load_with_equilibrium() {
        let system = LinearDynamicSystem {
            schema_version: DYNAMICS_SCHEMA_VERSION.to_owned(),
            analysis_id: Uuid::from_u128(10),
            dof_ids: vec![Uuid::from_u128(11)],
            mass_matrix_kg: vec![vec![1.0]],
            stiffness_matrix_n_per_m: vec![vec![100.0]],
        };
        let input = TimeHistoryAnalysisInput {
            system,
            damping: DampingModel::None,
            loads: (0..=10)
                .map(|i| TimeHistoryLoadPoint {
                    time_s: i as f64 * 0.01,
                    force_n: vec![1.0],
                })
                .collect(),
            initial_displacement_m: vec![0.0],
            initial_velocity_m_per_s: vec![0.0],
        };
        let result = ReferenceDynamicSolver
            .solve_time_history(&input, TimeHistoryExecutionOptions::default())
            .unwrap();
        assert_eq!(result.steps.len(), 11);
        assert!(result.steps.iter().all(|step| step.equilibrium_residual_norm_n < 1.0e-10));
        assert!(result.steps.last().unwrap().displacement_m[0] > 0.0);
    }

    #[test]
    fn repeated_modal_runs_are_identical() {
        let a = ReferenceDynamicSolver
            .solve_modal(&diagonal_system(), EigenExecutionOptions::default())
            .unwrap();
        let b = ReferenceDynamicSolver
            .solve_modal(&diagonal_system(), EigenExecutionOptions::default())
            .unwrap();
        assert_eq!(a, b);
    }
}
