use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use structural_domain::{AnalysisInput, AnalysisResult};
use uuid::Uuid;

pub const NONLINEAR_SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, Copy)]
pub struct ExecutionOptions {
    pub relative_tolerance: f64,
    pub deterministic_profile: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            relative_tolerance: 1.0e-9,
            deterministic_profile: true,
        }
    }
}

pub trait StructuralSolver: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn solve(&self, input: &AnalysisInput, options: ExecutionOptions)
        -> Result<AnalysisResult>;
}

/// A deliberately small, auditable nonlinear problem: independent translational
/// spring DOFs. It exercises the nonlinear solution protocol without pretending
/// to be a general-purpose nonlinear finite-element formulation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearAnalysisInput {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub dofs: Vec<NonlinearSpringDof>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearSpringDof {
    pub id: Uuid,
    pub node_id: Uuid,
    /// Initial elastic tangent in N/m.
    pub elastic_stiffness_n_per_m: f64,
    /// Optional symmetric yield force in N. `None` disables material nonlinearity.
    pub yield_force_n: Option<f64>,
    /// Ratio of post-yield to initial tangent, in [0, 1).
    pub post_yield_stiffness_ratio: f64,
    /// Cubic geometric stiffness coefficient in N/m^3. May be negative, but the
    /// current tangent must remain positive throughout the solved path.
    pub geometric_cubic_stiffness_n_per_m3: f64,
    /// Reference external force at load factor 1.0, in N.
    pub reference_force_n: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct NonlinearExecutionOptions {
    pub target_load_factor: f64,
    pub initial_load_increment: f64,
    pub minimum_load_increment: f64,
    pub maximum_load_increment: f64,
    pub maximum_steps: u32,
    pub maximum_iterations_per_step: u32,
    pub maximum_cutbacks: u32,
    pub residual_absolute_tolerance_n: f64,
    pub residual_relative_tolerance: f64,
    pub displacement_increment_tolerance_m: f64,
    pub restart_interval: u32,
    pub deterministic_profile: bool,
}

impl Default for NonlinearExecutionOptions {
    fn default() -> Self {
        Self {
            target_load_factor: 1.0,
            initial_load_increment: 0.1,
            minimum_load_increment: 1.0e-4,
            maximum_load_increment: 0.25,
            maximum_steps: 100,
            maximum_iterations_per_step: 25,
            maximum_cutbacks: 12,
            residual_absolute_tolerance_n: 1.0e-8,
            residual_relative_tolerance: 1.0e-9,
            displacement_increment_tolerance_m: 1.0e-12,
            restart_interval: 1,
            deterministic_profile: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearRestartPoint {
    pub schema_version: String,
    pub solver_id: String,
    pub solver_version: String,
    pub analysis_id: Uuid,
    pub converged_load_factor: f64,
    pub completed_steps: u32,
    pub dof_states: Vec<NonlinearDofState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearDofState {
    pub dof_id: Uuid,
    pub displacement_m: f64,
    pub plastic_displacement_m: f64,
    pub accumulated_plastic_displacement_m: f64,
    pub internal_force_n: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearIterationDiagnostic {
    pub step: u32,
    pub attempt: u32,
    pub iteration: u32,
    pub target_load_factor: f64,
    pub residual_norm_n: f64,
    pub displacement_increment_norm_m: f64,
    pub minimum_tangent_n_per_m: f64,
    pub converged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearStepResult {
    pub step: u32,
    pub load_factor: f64,
    pub load_increment: f64,
    pub iterations: u32,
    pub cutbacks_before_acceptance: u32,
    pub residual_norm_n: f64,
    pub displacement_increment_norm_m: f64,
    pub dof_states: Vec<NonlinearDofState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NonlinearTermination {
    Converged,
    StepLimit,
    IncrementBelowMinimum,
    CutbackLimit,
    NonPositiveTangent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NonlinearAnalysisResult {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub solver_id: String,
    pub solver_version: String,
    pub termination: NonlinearTermination,
    pub converged_load_factor: f64,
    pub steps: Vec<NonlinearStepResult>,
    pub diagnostics: Vec<NonlinearIterationDiagnostic>,
    pub restart_points: Vec<NonlinearRestartPoint>,
    pub final_states: Vec<NonlinearDofState>,
    pub warnings: Vec<String>,
}

impl NonlinearAnalysisResult {
    pub fn converged(&self) -> bool {
        self.termination == NonlinearTermination::Converged
    }
}

impl NonlinearAnalysisInput {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != NONLINEAR_SCHEMA_VERSION {
            bail!(
                "nonlinear input schema must be {}, got {}",
                NONLINEAR_SCHEMA_VERSION,
                self.schema_version
            );
        }
        if self.dofs.is_empty() {
            bail!("nonlinear input must contain at least one DOF");
        }
        let mut ids = std::collections::HashSet::new();
        for dof in &self.dofs {
            if !ids.insert(dof.id) {
                bail!("duplicate nonlinear DOF identifier {}", dof.id);
            }
            if !dof.elastic_stiffness_n_per_m.is_finite()
                || dof.elastic_stiffness_n_per_m <= 0.0
            {
                bail!("DOF {} requires positive finite elastic stiffness", dof.id);
            }
            if let Some(force) = dof.yield_force_n {
                if !force.is_finite() || force <= 0.0 {
                    bail!("DOF {} requires positive finite yield force", dof.id);
                }
            }
            if !dof.post_yield_stiffness_ratio.is_finite()
                || !(0.0..1.0).contains(&dof.post_yield_stiffness_ratio)
            {
                bail!("DOF {} post-yield stiffness ratio must be in [0, 1)", dof.id);
            }
            if dof.yield_force_n.is_none() && dof.post_yield_stiffness_ratio != 0.0 {
                bail!("DOF {} has hardening but no yield force", dof.id);
            }
            if !dof.geometric_cubic_stiffness_n_per_m3.is_finite()
                || !dof.reference_force_n.is_finite()
            {
                bail!("DOF {} contains a non-finite coefficient or load", dof.id);
            }
        }
        Ok(())
    }
}

impl NonlinearExecutionOptions {
    pub fn validate(&self) -> Result<()> {
        let finite = [
            self.target_load_factor,
            self.initial_load_increment,
            self.minimum_load_increment,
            self.maximum_load_increment,
            self.residual_absolute_tolerance_n,
            self.residual_relative_tolerance,
            self.displacement_increment_tolerance_m,
        ]
        .iter()
        .all(|value| value.is_finite());
        if !finite {
            bail!("nonlinear execution options must be finite");
        }
        if self.target_load_factor <= 0.0 {
            bail!("target load factor must be positive");
        }
        if self.minimum_load_increment <= 0.0
            || self.initial_load_increment < self.minimum_load_increment
            || self.maximum_load_increment < self.initial_load_increment
        {
            bail!("load increments must satisfy 0 < minimum <= initial <= maximum");
        }
        if self.maximum_steps == 0 || self.maximum_iterations_per_step == 0 {
            bail!("step and iteration limits must be non-zero");
        }
        if self.residual_absolute_tolerance_n < 0.0
            || self.residual_relative_tolerance < 0.0
            || self.displacement_increment_tolerance_m < 0.0
        {
            bail!("convergence tolerances must be non-negative");
        }
        if self.restart_interval == 0 {
            bail!("restart interval must be non-zero");
        }
        Ok(())
    }
}

pub trait NonlinearStructuralSolver: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn solve_nonlinear(
        &self,
        input: &NonlinearAnalysisInput,
        options: NonlinearExecutionOptions,
        restart: Option<&NonlinearRestartPoint>,
    ) -> Result<NonlinearAnalysisResult>;
}


pub const DYNAMICS_SCHEMA_VERSION: &str = "0.1";

/// Dense, symmetric reference system used by Pass 16. Production sparse assembly
/// remains behind this versioned boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinearDynamicSystem {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub dof_ids: Vec<Uuid>,
    /// Symmetric mass matrix in kg.
    pub mass_matrix_kg: Vec<Vec<f64>>,
    /// Symmetric elastic stiffness matrix in N/m.
    pub stiffness_matrix_n_per_m: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModeNormalization {
    Mass,
    MaximumAbsoluteComponent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct EigenExecutionOptions {
    pub requested_modes: usize,
    pub symmetry_tolerance: f64,
    pub positive_definite_tolerance: f64,
    pub jacobi_tolerance: f64,
    pub residual_tolerance: f64,
    pub maximum_sweeps: u32,
    pub normalization: ModeNormalization,
    pub deterministic_profile: bool,
}

impl Default for EigenExecutionOptions {
    fn default() -> Self {
        Self {
            requested_modes: 10,
            symmetry_tolerance: 1.0e-12,
            positive_definite_tolerance: 1.0e-14,
            jacobi_tolerance: 1.0e-12,
            residual_tolerance: 1.0e-8,
            maximum_sweeps: 100,
            normalization: ModeNormalization::Mass,
            deterministic_profile: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SolverToleranceDeclaration {
    pub symmetry_tolerance: f64,
    pub positive_definite_tolerance: f64,
    pub iteration_tolerance: f64,
    pub residual_tolerance: f64,
    pub maximum_iterations: u32,
    pub deterministic_profile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EigenMode {
    pub mode_number: usize,
    pub eigenvalue: f64,
    pub circular_frequency_rad_per_s: Option<f64>,
    pub frequency_hz: Option<f64>,
    pub vector: Vec<f64>,
    pub generalized_mass: f64,
    pub generalized_stiffness: f64,
    pub residual_norm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModalAnalysisResult {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub solver_id: String,
    pub solver_version: String,
    pub normalization: ModeNormalization,
    pub tolerances: SolverToleranceDeclaration,
    pub modes: Vec<EigenMode>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BucklingAnalysisInput {
    pub system: LinearDynamicSystem,
    /// Reference geometric stiffness matrix. The Pass 16 dense reference solver
    /// requires this matrix to be symmetric positive definite and solves
    /// K phi = lambda Kg phi.
    pub geometric_stiffness_n_per_m: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BucklingMode {
    pub mode_number: usize,
    pub load_factor: f64,
    pub vector: Vec<f64>,
    pub elastic_energy_measure: f64,
    pub geometric_energy_measure: f64,
    pub residual_norm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BucklingAnalysisResult {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub solver_id: String,
    pub solver_version: String,
    pub normalization: ModeNormalization,
    pub tolerances: SolverToleranceDeclaration,
    pub modes: Vec<BucklingMode>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DampingModel {
    None,
    /// C = alpha M + beta K.
    Rayleigh {
        mass_coefficient_per_s: f64,
        stiffness_coefficient_s: f64,
    },
    /// A convenient single-frequency viscous model:
    /// C = 2 zeta omega_ref M.
    ReferenceCriticalRatio {
        critical_ratio: f64,
        reference_frequency_hz: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeHistoryLoadPoint {
    pub time_s: f64,
    pub force_n: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeHistoryAnalysisInput {
    pub system: LinearDynamicSystem,
    pub damping: DampingModel,
    pub loads: Vec<TimeHistoryLoadPoint>,
    pub initial_displacement_m: Vec<f64>,
    pub initial_velocity_m_per_s: Vec<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TimeHistoryExecutionOptions {
    /// Newmark parameter. Average acceleration uses beta=0.25.
    pub newmark_beta: f64,
    /// Newmark parameter. Average acceleration uses gamma=0.5.
    pub newmark_gamma: f64,
    pub matrix_symmetry_tolerance: f64,
    pub pivot_tolerance: f64,
    pub equilibrium_residual_tolerance: f64,
    pub deterministic_profile: bool,
}

impl Default for TimeHistoryExecutionOptions {
    fn default() -> Self {
        Self {
            newmark_beta: 0.25,
            newmark_gamma: 0.5,
            matrix_symmetry_tolerance: 1.0e-12,
            pivot_tolerance: 1.0e-14,
            equilibrium_residual_tolerance: 1.0e-8,
            deterministic_profile: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeHistoryStepResult {
    pub step: usize,
    pub time_s: f64,
    pub displacement_m: Vec<f64>,
    pub velocity_m_per_s: Vec<f64>,
    pub acceleration_m_per_s2: Vec<f64>,
    pub applied_force_n: Vec<f64>,
    pub equilibrium_residual_norm_n: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeHistoryAnalysisResult {
    pub schema_version: String,
    pub analysis_id: Uuid,
    pub solver_id: String,
    pub solver_version: String,
    pub damping: DampingModel,
    pub newmark_beta: f64,
    pub newmark_gamma: f64,
    pub tolerances: SolverToleranceDeclaration,
    pub steps: Vec<TimeHistoryStepResult>,
    pub warnings: Vec<String>,
}

impl LinearDynamicSystem {
    pub fn validate(&self, symmetry_tolerance: f64) -> Result<()> {
        if self.schema_version != DYNAMICS_SCHEMA_VERSION {
            bail!(
                "dynamic system schema must be {}, got {}",
                DYNAMICS_SCHEMA_VERSION,
                self.schema_version
            );
        }
        let n = self.dof_ids.len();
        if n == 0 {
            bail!("dynamic system must contain at least one DOF");
        }
        let unique: std::collections::HashSet<_> = self.dof_ids.iter().collect();
        if unique.len() != n {
            bail!("dynamic system contains duplicate DOF identifiers");
        }
        validate_symmetric_matrix(&self.mass_matrix_kg, n, symmetry_tolerance, "mass")?;
        validate_symmetric_matrix(
            &self.stiffness_matrix_n_per_m,
            n,
            symmetry_tolerance,
            "stiffness",
        )?;
        Ok(())
    }
}

impl EigenExecutionOptions {
    pub fn validate(&self) -> Result<()> {
        if self.requested_modes == 0 || self.maximum_sweeps == 0 {
            bail!("requested modes and maximum sweeps must be non-zero");
        }
        for (name, value) in [
            ("symmetry tolerance", self.symmetry_tolerance),
            ("positive-definite tolerance", self.positive_definite_tolerance),
            ("Jacobi tolerance", self.jacobi_tolerance),
            ("residual tolerance", self.residual_tolerance),
        ] {
            if !value.is_finite() || value < 0.0 {
                bail!("{} must be finite and non-negative", name);
            }
        }
        Ok(())
    }
}

impl BucklingAnalysisInput {
    pub fn validate(&self, symmetry_tolerance: f64) -> Result<()> {
        self.system.validate(symmetry_tolerance)?;
        validate_symmetric_matrix(
            &self.geometric_stiffness_n_per_m,
            self.system.dof_ids.len(),
            symmetry_tolerance,
            "geometric stiffness",
        )
    }
}

impl TimeHistoryAnalysisInput {
    pub fn validate(&self, symmetry_tolerance: f64) -> Result<()> {
        self.system.validate(symmetry_tolerance)?;
        let n = self.system.dof_ids.len();
        if self.loads.is_empty() {
            bail!("time history requires at least one load point");
        }
        if self.initial_displacement_m.len() != n || self.initial_velocity_m_per_s.len() != n {
            bail!("initial condition vector size must equal the DOF count");
        }
        if self
            .initial_displacement_m
            .iter()
            .chain(&self.initial_velocity_m_per_s)
            .any(|value| !value.is_finite())
        {
            bail!("initial conditions must be finite");
        }
        let mut previous = None;
        for point in &self.loads {
            if !point.time_s.is_finite()
                || point.force_n.len() != n
                || point.force_n.iter().any(|value| !value.is_finite())
            {
                bail!("time-history load point is non-finite or has the wrong size");
            }
            if let Some(time) = previous {
                if point.time_s <= time {
                    bail!("time-history load times must be strictly increasing");
                }
            }
            previous = Some(point.time_s);
        }
        if self.loads[0].time_s != 0.0 {
            bail!("the first time-history load point must be at time zero");
        }
        match &self.damping {
            DampingModel::None => {}
            DampingModel::Rayleigh {
                mass_coefficient_per_s,
                stiffness_coefficient_s,
            } => {
                if !mass_coefficient_per_s.is_finite()
                    || !stiffness_coefficient_s.is_finite()
                    || *mass_coefficient_per_s < 0.0
                    || *stiffness_coefficient_s < 0.0
                {
                    bail!("Rayleigh damping coefficients must be finite and non-negative");
                }
            }
            DampingModel::ReferenceCriticalRatio {
                critical_ratio,
                reference_frequency_hz,
            } => {
                if !critical_ratio.is_finite()
                    || !reference_frequency_hz.is_finite()
                    || *critical_ratio < 0.0
                    || *reference_frequency_hz <= 0.0
                {
                    bail!("critical ratio must be non-negative and reference frequency positive");
                }
            }
        }
        Ok(())
    }
}

impl TimeHistoryExecutionOptions {
    pub fn validate(&self) -> Result<()> {
        if !self.newmark_beta.is_finite()
            || !self.newmark_gamma.is_finite()
            || self.newmark_beta <= 0.0
            || self.newmark_gamma <= 0.0
        {
            bail!("Newmark beta and gamma must be finite and positive");
        }
        if self.newmark_gamma < 0.5 || self.newmark_beta < 0.25 * (self.newmark_gamma + 0.5).powi(2) {
            bail!("Newmark parameters do not satisfy the declared unconditional-stability limit");
        }
        for value in [
            self.matrix_symmetry_tolerance,
            self.pivot_tolerance,
            self.equilibrium_residual_tolerance,
        ] {
            if !value.is_finite() || value < 0.0 {
                bail!("time-history tolerances must be finite and non-negative");
            }
        }
        Ok(())
    }
}

fn validate_symmetric_matrix(
    matrix: &[Vec<f64>],
    expected: usize,
    tolerance: f64,
    name: &str,
) -> Result<()> {
    if matrix.len() != expected || matrix.iter().any(|row| row.len() != expected) {
        bail!("{} matrix must be square with size {}", name, expected);
    }
    for i in 0..expected {
        for j in 0..expected {
            if !matrix[i][j].is_finite() {
                bail!("{} matrix contains non-finite entries", name);
            }
            let scale = matrix[i][j].abs().max(matrix[j][i].abs()).max(1.0);
            if (matrix[i][j] - matrix[j][i]).abs() > tolerance * scale {
                bail!("{} matrix is not symmetric within tolerance", name);
            }
        }
    }
    Ok(())
}

pub trait DynamicStructuralSolver: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn solve_modal(
        &self,
        system: &LinearDynamicSystem,
        options: EigenExecutionOptions,
    ) -> Result<ModalAnalysisResult>;
    fn solve_buckling(
        &self,
        input: &BucklingAnalysisInput,
        options: EigenExecutionOptions,
    ) -> Result<BucklingAnalysisResult>;
    fn solve_time_history(
        &self,
        input: &TimeHistoryAnalysisInput,
        options: TimeHistoryExecutionOptions,
    ) -> Result<TimeHistoryAnalysisResult>;
}
