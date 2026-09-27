use anyhow::{bail, Context, Result};
use structural_domain::{AnalysisInput, AnalysisResult, NodeResult};
use structural_solver_api::{ExecutionOptions, StructuralSolver};
use structural_units::{Force, Stiffness};

/// Demonstrator only: solves independent global-X one-DOF springs using u = F / k.
pub struct LinearSpringSolver;

impl StructuralSolver for LinearSpringSolver {
    fn id(&self) -> &'static str {
        "sweco.structural.linear-spring-demo"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn solve(
        &self,
        input: &AnalysisInput,
        _options: ExecutionOptions,
    ) -> Result<AnalysisResult> {
        input.validate_or_error()?;

        let load_case_ids: Vec<_> = if input.selected_load_case_ids.is_empty() {
            input.model.load_cases.iter().map(|x| x.id).collect()
        } else {
            input.selected_load_case_ids.clone()
        };
        if load_case_ids.len() != 1 {
            bail!("demonstrator solver requires exactly one selected load case");
        }
        let load_case = input
            .model
            .load_cases
            .iter()
            .find(|x| x.id == load_case_ids[0])
            .context("selected load case disappeared after validation")?;

        let mut results = Vec::with_capacity(input.model.nodes.len());
        for node in &input.model.nodes {
            let stiffness = input
                .model
                .springs
                .iter()
                .filter(|s| s.node_id == node.id)
                .fold(Stiffness::ZERO, |sum, s| sum + s.stiffness_n_per_m);

            if !stiffness.is_finite() || stiffness.newtons_per_metre() <= 0.0 {
                bail!("node {} has no positive finite X stiffness", node.id);
            }

            let force = load_case
                .nodal_loads
                .iter()
                .filter(|load| load.node_id == node.id)
                .fold(Force::ZERO, |sum, load| sum + load.force_n[0]);

            let displacement = force / stiffness;
            if !displacement.is_finite() {
                bail!("non-finite displacement at node {}", node.id);
            }

            results.push(NodeResult {
                node_id: node.id,
                displacement_m: displacement,
                reaction_n: -force,
            });
        }

        Ok(AnalysisResult {
            schema_version: input.schema_version.clone(),
            solver_id: self.id().to_owned(),
            solver_version: self.version().to_owned(),
            warnings: vec![
                "Demonstrator solver: independent global-X 1-DOF springs only.".to_owned(),
                "Members, shells, solids, supports and combinations are not solved in Pass 4."
                    .to_owned(),
            ],
            nodes: results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use structural_domain::{
        AnalysisInput, CoordinateSystem, LoadCase, LoadCaseCategory, NodalLoad, Node,
        SpringElement, StructuralModel, MODEL_SCHEMA_VERSION,
    };
    use structural_units::{Force, Length, Moment, Stiffness};
    use uuid::Uuid;

    #[test]
    fn solves_hookes_law_from_pass3_load_case() {
        let node_id = Uuid::new_v4();
        let cs_id = Uuid::new_v4();
        let load_case_id = Uuid::new_v4();
        let input = AnalysisInput {
            schema_version: MODEL_SCHEMA_VERSION.into(),
            model: StructuralModel {
                model_id: Uuid::new_v4(),
                revision_id: Uuid::new_v4(),
                name: "Solver test".to_owned(),
                global_coordinate_system_id: cs_id,
                coordinate_systems: vec![CoordinateSystem::global(cs_id)],
                nodes: vec![Node {
                    id: node_id,
                    name: None,
                    xyz_m: [Length::ZERO; 3],
                    coordinate_system_id: None,
                    provenance: None,
                }],
                materials: vec![],
                sections: vec![],
                members: vec![],
                shells: vec![],
                solids: vec![],
                springs: vec![SpringElement {
                    id: Uuid::new_v4(),
                    node_id,
                    stiffness_n_per_m: Stiffness::from_newtons_per_metre(20_000.0),
                    provenance: None,
                }],
                supports: vec![],
                load_cases: vec![LoadCase {
                    id: load_case_id,
                    name: "LC1".to_owned(),
                    category: LoadCaseCategory::Other,
                    self_weight_factor: 0.0,
                    nodal_loads: vec![NodalLoad {
                        id: Uuid::new_v4(),
                        node_id,
                        coordinate_system_id: None,
                        force_n: [Force::from_newtons(1_000.0), Force::ZERO, Force::ZERO],
                        moment_nm: [Moment::ZERO; 3],
                        provenance: None,
                    }],
                    provenance: None,
                }],
                load_combinations: vec![],
                provenance: None,
            },
            selected_load_case_ids: vec![load_case_id],
            rule_sets: vec![],
        };

        let result = LinearSpringSolver
            .solve(&input, ExecutionOptions::default())
            .unwrap();

        assert!((result.nodes[0].displacement_m.metres() - 0.05).abs() < 1.0e-12);
        assert_eq!(result.nodes[0].reaction_n.newtons(), -1_000.0);
    }
}
