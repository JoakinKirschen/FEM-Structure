//! Deterministic reference nonlinear solver for independent one-DOF springs.
//!
//! This crate validates load stepping, Newton convergence, material/geometric
//! nonlinearity, restart state and diagnostics. It is not a general nonlinear FEM solver.

use anyhow::{bail, Result};
use structural_solver_api::{
    NonlinearAnalysisInput, NonlinearAnalysisResult, NonlinearDofState,
    NonlinearExecutionOptions, NonlinearIterationDiagnostic, NonlinearRestartPoint,
    NonlinearStepResult, NonlinearStructuralSolver, NonlinearTermination,
    NONLINEAR_SCHEMA_VERSION,
};

const TANGENT_FLOOR: f64 = 1.0e-14;

pub struct ReferenceNonlinearSpringSolver;

#[derive(Clone, Copy)]
struct Response {
    internal_force: f64,
    tangent: f64,
    plastic_displacement: f64,
    accumulated_plastic_displacement: f64,
}

impl ReferenceNonlinearSpringSolver {
    fn response(
        dof: &structural_solver_api::NonlinearSpringDof,
        displacement: f64,
        committed: &NonlinearDofState,
    ) -> Response {
        let k = dof.elastic_stiffness_n_per_m;
        let (material_force, material_tangent, plastic_displacement, accumulated) =
            if let Some(yield_force) = dof.yield_force_n {
                let ratio = dof.post_yield_stiffness_ratio;
                let hardening_modulus = if ratio == 0.0 {
                    0.0
                } else {
                    ratio * k / (1.0 - ratio)
                };
                let trial = k * (displacement - committed.plastic_displacement_m);
                let yield_limit =
                    yield_force + hardening_modulus * committed.accumulated_plastic_displacement_m;
                let yield_function = trial.abs() - yield_limit;
                if yield_function <= 0.0 {
                    (
                        trial,
                        k,
                        committed.plastic_displacement_m,
                        committed.accumulated_plastic_displacement_m,
                    )
                } else {
                    let direction = trial.signum();
                    let plastic_increment = yield_function / (k + hardening_modulus);
                    let plastic_displacement =
                        committed.plastic_displacement_m + direction * plastic_increment;
                    let accumulated =
                        committed.accumulated_plastic_displacement_m + plastic_increment;
                    let force = trial - k * direction * plastic_increment;
                    let tangent = if hardening_modulus == 0.0 {
                        0.0
                    } else {
                        k * hardening_modulus / (k + hardening_modulus)
                    };
                    (force, tangent, plastic_displacement, accumulated)
                }
            } else {
                (k * displacement, k, 0.0, 0.0)
            };

        let geometric_force =
            dof.geometric_cubic_stiffness_n_per_m3 * displacement.powi(3);
        let geometric_tangent =
            3.0 * dof.geometric_cubic_stiffness_n_per_m3 * displacement.powi(2);

        Response {
            internal_force: material_force + geometric_force,
            tangent: material_tangent + geometric_tangent,
            plastic_displacement,
            accumulated_plastic_displacement: accumulated,
        }
    }

    fn initial_states(input: &NonlinearAnalysisInput) -> Vec<NonlinearDofState> {
        input
            .dofs
            .iter()
            .map(|dof| NonlinearDofState {
                dof_id: dof.id,
                displacement_m: 0.0,
                plastic_displacement_m: 0.0,
                accumulated_plastic_displacement_m: 0.0,
                internal_force_n: 0.0,
            })
            .collect()
    }

    fn validate_restart(
        &self,
        input: &NonlinearAnalysisInput,
        options: NonlinearExecutionOptions,
        restart: &NonlinearRestartPoint,
    ) -> Result<()> {
        if restart.schema_version != NONLINEAR_SCHEMA_VERSION {
            bail!("restart schema version is not supported");
        }
        if restart.solver_id != self.id() {
            bail!("restart was produced by a different solver");
        }
        if restart.solver_version != self.version() {
            bail!("restart was produced by a different solver version");
        }
        if restart.analysis_id != input.analysis_id {
            bail!("restart analysis_id does not match input");
        }
        if !restart.converged_load_factor.is_finite()
            || restart.converged_load_factor < 0.0
            || restart.converged_load_factor > options.target_load_factor
        {
            bail!("restart load factor is outside the requested path");
        }
        if restart.dof_states.len() != input.dofs.len() {
            bail!("restart DOF count does not match input");
        }
        for (dof, state) in input.dofs.iter().zip(&restart.dof_states) {
            if state.dof_id != dof.id {
                bail!("restart DOF ordering or identity does not match input");
            }
            if [
                state.displacement_m,
                state.plastic_displacement_m,
                state.accumulated_plastic_displacement_m,
                state.internal_force_n,
            ]
            .iter()
            .any(|value| !value.is_finite())
            {
                bail!("restart contains non-finite state");
            }
            if state.accumulated_plastic_displacement_m < 0.0 {
                bail!("restart contains negative accumulated plastic displacement");
            }
        }
        Ok(())
    }
}

impl NonlinearStructuralSolver for ReferenceNonlinearSpringSolver {
    fn id(&self) -> &'static str {
        "sweco.structural.nonlinear-spring-reference"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn solve_nonlinear(
        &self,
        input: &NonlinearAnalysisInput,
        options: NonlinearExecutionOptions,
        restart: Option<&NonlinearRestartPoint>,
    ) -> Result<NonlinearAnalysisResult> {
        input.validate()?;
        options.validate()?;

        let (mut load_factor, completed_steps, mut committed) = if let Some(point) = restart {
            self.validate_restart(input, options, point)?;
            (
                point.converged_load_factor,
                point.completed_steps,
                point.dof_states.clone(),
            )
        } else {
            (0.0, 0, Self::initial_states(input))
        };

        let mut increment = options
            .initial_load_increment
            .min(options.target_load_factor - load_factor);
        let mut steps = Vec::new();
        let mut diagnostics = Vec::new();
        let mut restart_points = Vec::new();
        let mut termination = NonlinearTermination::Converged;

        while load_factor + f64::EPSILON < options.target_load_factor {
            if steps.len() as u32 >= options.maximum_steps {
                termination = NonlinearTermination::StepLimit;
                break;
            }

            let remaining = options.target_load_factor - load_factor;
            increment = increment.min(remaining).min(options.maximum_load_increment);
            let mut accepted = false;
            let mut cutbacks = 0_u32;

            while !accepted {
                if increment + f64::EPSILON < options.minimum_load_increment {
                    termination = NonlinearTermination::IncrementBelowMinimum;
                    break;
                }

                let target = load_factor + increment;
                let mut trial_displacements: Vec<f64> =
                    committed.iter().map(|state| state.displacement_m).collect();
                let mut accepted_states = committed.clone();
                let mut last_increment_norm = 0.0;
                let mut last_residual_norm = f64::INFINITY;
                let mut converged_iteration = None;
                let mut bad_tangent = false;

                for iteration in 1..=options.maximum_iterations_per_step {
                    let mut residual_norm: f64 = 0.0;
                    let mut minimum_tangent = f64::INFINITY;
                    let mut responses = Vec::with_capacity(input.dofs.len());

                    for ((dof, committed_state), displacement) in input
                        .dofs
                        .iter()
                        .zip(&committed)
                        .zip(&trial_displacements)
                    {
                        let response = Self::response(dof, *displacement, committed_state);
                        let external = target * dof.reference_force_n;
                        residual_norm =
                            residual_norm.max((external - response.internal_force).abs());
                        minimum_tangent = minimum_tangent.min(response.tangent);
                        responses.push(response);
                    }

                    let force_scale = input
                        .dofs
                        .iter()
                        .map(|dof| (target * dof.reference_force_n).abs())
                        .fold(1.0_f64, f64::max);
                    let residual_limit = options.residual_absolute_tolerance_n
                        + options.residual_relative_tolerance * force_scale;
                    let displacement_ok = iteration == 1
                        || last_increment_norm <= options.displacement_increment_tolerance_m;
                    let converged = residual_norm <= residual_limit && displacement_ok;

                    diagnostics.push(NonlinearIterationDiagnostic {
                        step: completed_steps + steps.len() as u32 + 1,
                        attempt: cutbacks + 1,
                        iteration,
                        target_load_factor: target,
                        residual_norm_n: residual_norm,
                        displacement_increment_norm_m: last_increment_norm,
                        minimum_tangent_n_per_m: minimum_tangent,
                        converged,
                    });

                    last_residual_norm = residual_norm;
                    if converged {
                        for (((state, dof), displacement), response) in accepted_states
                            .iter_mut()
                            .zip(&input.dofs)
                            .zip(&trial_displacements)
                            .zip(&responses)
                        {
                            state.dof_id = dof.id;
                            state.displacement_m = *displacement;
                            state.plastic_displacement_m = response.plastic_displacement;
                            state.accumulated_plastic_displacement_m =
                                response.accumulated_plastic_displacement;
                            state.internal_force_n = response.internal_force;
                        }
                        converged_iteration = Some(iteration);
                        break;
                    }

                    if minimum_tangent <= TANGENT_FLOOR || !minimum_tangent.is_finite() {
                        bad_tangent = true;
                        break;
                    }

                    last_increment_norm = 0.0;
                    for ((dof, displacement), response) in input
                        .dofs
                        .iter()
                        .zip(trial_displacements.iter_mut())
                        .zip(&responses)
                    {
                        let residual =
                            target * dof.reference_force_n - response.internal_force;
                        let correction = residual / response.tangent;
                        if !correction.is_finite() {
                            bad_tangent = true;
                            break;
                        }
                        *displacement += correction;
                        last_increment_norm = last_increment_norm.max(correction.abs());
                    }
                    if bad_tangent {
                        break;
                    }
                }

                if let Some(iterations) = converged_iteration {
                    committed = accepted_states;
                    load_factor = target;
                    let step_number = completed_steps + steps.len() as u32 + 1;
                    steps.push(NonlinearStepResult {
                        step: step_number,
                        load_factor,
                        load_increment: increment,
                        iterations,
                        cutbacks_before_acceptance: cutbacks,
                        residual_norm_n: last_residual_norm,
                        displacement_increment_norm_m: last_increment_norm,
                        dof_states: committed.clone(),
                    });
                    if step_number % options.restart_interval == 0
                        || load_factor + f64::EPSILON >= options.target_load_factor
                    {
                        restart_points.push(NonlinearRestartPoint {
                            schema_version: NONLINEAR_SCHEMA_VERSION.to_owned(),
                            solver_id: self.id().to_owned(),
                            solver_version: self.version().to_owned(),
                            analysis_id: input.analysis_id,
                            converged_load_factor: load_factor,
                            completed_steps: step_number,
                            dof_states: committed.clone(),
                        });
                    }
                    increment = if iterations <= 4 {
                        (increment * 1.5).min(options.maximum_load_increment)
                    } else {
                        increment
                    };
                    accepted = true;
                } else {
                    cutbacks += 1;
                    if cutbacks > options.maximum_cutbacks {
                        termination = if bad_tangent {
                            NonlinearTermination::NonPositiveTangent
                        } else {
                            NonlinearTermination::CutbackLimit
                        };
                        break;
                    }
                    increment *= 0.5;
                }
            }

            if !accepted {
                break;
            }
        }

        if load_factor + f64::EPSILON < options.target_load_factor
            && termination == NonlinearTermination::Converged
        {
            termination = NonlinearTermination::StepLimit;
        }

        Ok(NonlinearAnalysisResult {
            schema_version: NONLINEAR_SCHEMA_VERSION.to_owned(),
            analysis_id: input.analysis_id,
            solver_id: self.id().to_owned(),
            solver_version: self.version().to_owned(),
            termination,
            converged_load_factor: load_factor,
            steps,
            diagnostics,
            restart_points,
            final_states: committed,
            warnings: vec![
                concat!(
                    "Reference solver: independent one-DOF springs only; no coupling, ",
                    "contact, or instability continuation."
                )
                .to_owned(),
                "Negative tangent paths terminate; arc-length methods are outside Pass 15."
                    .to_owned(),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use structural_solver_api::{
        NonlinearAnalysisInput, NonlinearExecutionOptions, NonlinearSpringDof,
        NonlinearStructuralSolver, NONLINEAR_SCHEMA_VERSION,
    };
    use uuid::Uuid;

    fn input(k: f64, yield_force: Option<f64>, ratio: f64, cubic: f64, force: f64)
        -> NonlinearAnalysisInput
    {
        NonlinearAnalysisInput {
            schema_version: NONLINEAR_SCHEMA_VERSION.to_owned(),
            analysis_id: Uuid::from_u128(1),
            dofs: vec![NonlinearSpringDof {
                id: Uuid::from_u128(2),
                node_id: Uuid::from_u128(3),
                elastic_stiffness_n_per_m: k,
                yield_force_n: yield_force,
                post_yield_stiffness_ratio: ratio,
                geometric_cubic_stiffness_n_per_m3: cubic,
                reference_force_n: force,
            }],
        }
    }

    #[test]
    fn reproduces_linear_solution_with_load_steps() {
        let result = ReferenceNonlinearSpringSolver
            .solve_nonlinear(
                &input(1_000.0, None, 0.0, 0.0, 100.0),
                NonlinearExecutionOptions::default(),
                None,
            )
            .unwrap();
        assert!(result.converged());
        assert!((result.final_states[0].displacement_m - 0.1).abs() < 1.0e-10);
        assert!(!result.steps.is_empty());
        assert!(result.diagnostics.iter().all(|d| d.residual_norm_n.is_finite()));
    }

    #[test]
    fn solves_bilinear_material_benchmark() {
        let result = ReferenceNonlinearSpringSolver
            .solve_nonlinear(
                &input(1_000.0, Some(10.0), 0.1, 0.0, 20.0),
                NonlinearExecutionOptions::default(),
                None,
            )
            .unwrap();
        assert!(result.converged());
        assert!((result.final_states[0].displacement_m - 0.11).abs() < 1.0e-9);
        assert!(result.final_states[0].accumulated_plastic_displacement_m > 0.0);
    }

    #[test]
    fn solves_cubic_geometric_benchmark() {
        let mut options = NonlinearExecutionOptions::default();
        options.initial_load_increment = 0.2;
        let result = ReferenceNonlinearSpringSolver
            .solve_nonlinear(
                &input(1_000.0, None, 0.0, 100_000.0, 200.0),
                options,
                None,
            )
            .unwrap();
        assert!(result.converged());
        assert!((result.final_states[0].displacement_m - 0.1).abs() < 1.0e-9);
    }

    #[test]
    fn restart_matches_uninterrupted_solution() {
        let model = input(1_000.0, Some(10.0), 0.1, 10_000.0, 30.0);
        let full = ReferenceNonlinearSpringSolver
            .solve_nonlinear(&model, NonlinearExecutionOptions::default(), None)
            .unwrap();

        let mut half_options = NonlinearExecutionOptions::default();
        half_options.target_load_factor = 0.5;
        let half = ReferenceNonlinearSpringSolver
            .solve_nonlinear(&model, half_options, None)
            .unwrap();
        let restart = half.restart_points.last().unwrap();

        let resumed = ReferenceNonlinearSpringSolver
            .solve_nonlinear(
                &model,
                NonlinearExecutionOptions::default(),
                Some(restart),
            )
            .unwrap();
        assert!(resumed.converged());
        assert!(
            (resumed.final_states[0].displacement_m
                - full.final_states[0].displacement_m)
                .abs()
                < 1.0e-9
        );
    }

    #[test]
    fn rejects_restart_for_another_analysis() {
        let model = input(1_000.0, None, 0.0, 0.0, 100.0);
        let mut restart = NonlinearRestartPoint {
            schema_version: NONLINEAR_SCHEMA_VERSION.to_owned(),
            solver_id: ReferenceNonlinearSpringSolver.id().to_owned(),
            solver_version: ReferenceNonlinearSpringSolver.version().to_owned(),
            analysis_id: Uuid::from_u128(999),
            converged_load_factor: 0.5,
            completed_steps: 1,
            dof_states: vec![NonlinearDofState {
                dof_id: Uuid::from_u128(2),
                displacement_m: 0.05,
                plastic_displacement_m: 0.0,
                accumulated_plastic_displacement_m: 0.0,
                internal_force_n: 50.0,
            }],
        };
        let error = ReferenceNonlinearSpringSolver
            .solve_nonlinear(
                &model,
                NonlinearExecutionOptions::default(),
                Some(&restart),
            )
            .unwrap_err();
        assert!(error.to_string().contains("analysis_id"));
        restart.analysis_id = model.analysis_id;
    }
}
