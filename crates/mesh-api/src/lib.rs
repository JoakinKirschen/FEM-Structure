//! Stable mesh-generation and mesh-quality contracts.
//!
//! Pass 8 preserves the Pass 7 triangular surface API and adds a solver-neutral
//! finite-element mesh model for line, triangular, quadrilateral, tetrahedral,
//! wedge, pyramid and hexahedral elements.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use structural_geometry_api::{GeometryDocument, GeometryProvenance, Point3, TriangleMesh};
use structural_units::Length;
use uuid::Uuid;

pub const MESH_SCHEMA_VERSION: &str = "0.3";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SurfaceMeshOptions {
    pub maximum_edge_length_m: Length,
    pub minimum_angle_deg: f64,
    pub maximum_aspect_ratio: f64,
    pub maximum_refinement_levels: u8,
    pub deterministic: bool,
}

impl SurfaceMeshOptions {
    pub fn engineering_default() -> Self {
        Self {
            maximum_edge_length_m: Length::from_metres(1.0),
            minimum_angle_deg: 15.0,
            maximum_aspect_ratio: 5.0,
            maximum_refinement_levels: 8,
            deterministic: true,
        }
    }

    pub fn is_valid(self) -> bool {
        self.maximum_edge_length_m.is_finite()
            && self.maximum_edge_length_m.metres() > 0.0
            && self.minimum_angle_deg.is_finite()
            && self.minimum_angle_deg > 0.0
            && self.minimum_angle_deg < 60.0
            && self.maximum_aspect_ratio.is_finite()
            && self.maximum_aspect_ratio >= 1.0
            && self.maximum_refinement_levels > 0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeshIssueSeverity {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshIssue {
    pub severity: MeshIssueSeverity,
    pub code: String,
    pub message: String,
    pub mesh_id: Uuid,
    pub triangle_id: Option<Uuid>,
    pub measured_value: Option<f64>,
    pub required_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshStatistics {
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub minimum_edge_length_m: Option<f64>,
    pub maximum_edge_length_m: Option<f64>,
    pub minimum_angle_deg: Option<f64>,
    pub maximum_aspect_ratio: Option<f64>,
    pub minimum_area_m2: Option<f64>,
    pub maximum_area_m2: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshQualityReport {
    pub schema_version: String,
    pub generator_id: String,
    pub generator_version: String,
    pub options: SurfaceMeshOptions,
    pub refinement_levels_used: u8,
    pub statistics: MeshStatistics,
    pub issues: Vec<MeshIssue>,
}

impl MeshQualityReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|issue| issue.severity == MeshIssueSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshGenerationOutcome {
    pub meshes: Vec<TriangleMesh>,
    pub report: MeshQualityReport,
}

pub trait SurfaceMeshGenerator: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn generate(
        &self,
        geometry: &GeometryDocument,
        options: SurfaceMeshOptions,
    ) -> Result<MeshGenerationOutcome>;

    fn assess(
        &self,
        meshes: &[TriangleMesh],
        options: SurfaceMeshOptions,
    ) -> MeshQualityReport;
}

// ---- Pass 8 generalized analysis mesh ------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Line2,
    Triangle3,
    Quadrilateral4,
    Tetrahedron4,
    Pyramid5,
    Wedge6,
    Hexahedron8,
}

impl ElementKind {
    pub fn dimension(self) -> u8 {
        match self {
            Self::Line2 => 1,
            Self::Triangle3 | Self::Quadrilateral4 => 2,
            Self::Tetrahedron4 | Self::Pyramid5 | Self::Wedge6 | Self::Hexahedron8 => 3,
        }
    }

    pub fn node_count(self) -> usize {
        match self {
            Self::Line2 => 2,
            Self::Triangle3 => 3,
            Self::Quadrilateral4 | Self::Tetrahedron4 => 4,
            Self::Pyramid5 => 5,
            Self::Wedge6 => 6,
            Self::Hexahedron8 => 8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisMeshNode {
    pub id: Uuid,
    pub point: Point3,
    pub source_vertex_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisElement {
    pub id: Uuid,
    pub kind: ElementKind,
    pub node_ids: Vec<Uuid>,
    pub source_geometry_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisMesh {
    pub schema_version: String,
    pub id: Uuid,
    pub nodes: Vec<AnalysisMeshNode>,
    pub elements: Vec<AnalysisElement>,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeshTarget {
    Line,
    Surface,
    ExtrudedVolume,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ExtrusionOptions {
    pub vector_m: [Length; 3],
    pub layers: u16,
}

impl ExtrusionOptions {
    pub fn is_valid(self) -> bool {
        self.layers > 0
            && self.vector_m.iter().all(|value| value.is_finite())
            && self.vector_m.iter().map(|value| value.metres().powi(2)).sum::<f64>() > 0.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GeneralMeshOptions {
    pub target: MeshTarget,
    pub maximum_edge_length_m: Length,
    pub maximum_aspect_ratio: f64,
    pub deterministic: bool,
    pub prefer_quadrilaterals: bool,
    pub extrusion: Option<ExtrusionOptions>,
}

impl GeneralMeshOptions {
    pub fn line_default() -> Self {
        Self {
            target: MeshTarget::Line,
            maximum_edge_length_m: Length::from_metres(1.0),
            maximum_aspect_ratio: 10.0,
            deterministic: true,
            prefer_quadrilaterals: false,
            extrusion: None,
        }
    }

    pub fn surface_default() -> Self {
        Self {
            target: MeshTarget::Surface,
            maximum_edge_length_m: Length::from_metres(1.0),
            maximum_aspect_ratio: 5.0,
            deterministic: true,
            prefer_quadrilaterals: true,
            extrusion: None,
        }
    }

    pub fn volume_default() -> Self {
        Self {
            target: MeshTarget::ExtrudedVolume,
            extrusion: Some(ExtrusionOptions {
                vector_m: [Length::ZERO, Length::ZERO, Length::from_metres(1.0)],
                layers: 1,
            }),
            ..Self::surface_default()
        }
    }

    pub fn is_valid(self) -> bool {
        self.maximum_edge_length_m.is_finite()
            && self.maximum_edge_length_m.metres() > 0.0
            && self.maximum_aspect_ratio.is_finite()
            && self.maximum_aspect_ratio >= 1.0
            && match self.target {
                MeshTarget::ExtrudedVolume => self.extrusion.map(|v| v.is_valid()).unwrap_or(false),
                _ => self.extrusion.is_none() || self.extrusion.map(|v| v.is_valid()).unwrap_or(false),
            }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElementQuality {
    pub element_id: Uuid,
    pub kind: ElementKind,
    pub minimum_edge_length_m: f64,
    pub maximum_edge_length_m: f64,
    pub aspect_ratio: f64,
    pub measure: f64,
    pub measure_unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralMeshIssue {
    pub severity: MeshIssueSeverity,
    pub code: String,
    pub message: String,
    pub mesh_id: Uuid,
    pub element_id: Option<Uuid>,
    pub measured_value: Option<f64>,
    pub required_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralMeshStatistics {
    pub mesh_count: usize,
    pub node_count: usize,
    pub element_count: usize,
    pub element_counts: BTreeMap<ElementKind, usize>,
    pub minimum_edge_length_m: Option<f64>,
    pub maximum_edge_length_m: Option<f64>,
    pub minimum_measure: Option<f64>,
    pub maximum_measure: Option<f64>,
    pub maximum_aspect_ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralMeshQualityReport {
    pub schema_version: String,
    pub generator_id: String,
    pub generator_version: String,
    pub options: GeneralMeshOptions,
    pub statistics: GeneralMeshStatistics,
    pub element_quality: Vec<ElementQuality>,
    pub issues: Vec<GeneralMeshIssue>,
}

impl GeneralMeshQualityReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|issue| issue.severity == MeshIssueSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralMeshGenerationOutcome {
    pub meshes: Vec<AnalysisMesh>,
    pub report: GeneralMeshQualityReport,
}

pub trait GeneralMeshGenerator: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn generate_general(
        &self,
        geometry: &GeometryDocument,
        options: GeneralMeshOptions,
    ) -> Result<GeneralMeshGenerationOutcome>;

    fn assess_general(
        &self,
        meshes: &[AnalysisMesh],
        options: GeneralMeshOptions,
    ) -> GeneralMeshQualityReport;
}


// ---- Pass 9 automatic tetrahedral volume-meshing boundary ----------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegionSeed {
    pub id: Uuid,
    pub point: Point3,
    pub material_region: Option<String>,
    pub maximum_element_volume_m3: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CavitySeed {
    pub id: Uuid,
    pub point: Point3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TetrahedralizationOptions {
    pub maximum_element_volume_m3: Option<f64>,
    pub maximum_edge_length_m: Option<Length>,
    pub minimum_dihedral_angle_deg: f64,
    pub minimum_scaled_jacobian: f64,
    pub merge_tolerance_m: Length,
    pub preserve_boundary: bool,
    pub allow_steiner_points: bool,
    pub maximum_refinement_levels: u8,
    pub require_single_region: bool,
    pub deterministic_seed: u64,
    pub region_seeds: Vec<RegionSeed>,
    pub cavity_seeds: Vec<CavitySeed>,
}

impl TetrahedralizationOptions {
    pub fn engineering_default() -> Self {
        Self {
            maximum_element_volume_m3: None,
            maximum_edge_length_m: None,
            minimum_dihedral_angle_deg: 5.0,
            minimum_scaled_jacobian: 0.02,
            merge_tolerance_m: Length::from_metres(1.0e-6),
            preserve_boundary: true,
            allow_steiner_points: true,
            maximum_refinement_levels: 8,
            require_single_region: true,
            deterministic_seed: 0,
            region_seeds: Vec::new(),
            cavity_seeds: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.maximum_element_volume_m3
            .map(|value| value.is_finite() && value > 0.0)
            .unwrap_or(true)
            && self.maximum_edge_length_m
                .map(|value| value.is_finite() && value.metres() > 0.0)
                .unwrap_or(true)
            && self.minimum_dihedral_angle_deg.is_finite()
            && self.minimum_dihedral_angle_deg > 0.0
            && self.minimum_dihedral_angle_deg < 90.0
            && self.minimum_scaled_jacobian.is_finite()
            && self.minimum_scaled_jacobian >= 0.0
            && self.minimum_scaled_jacobian <= 1.0
            && self.merge_tolerance_m.is_finite()
            && self.merge_tolerance_m.metres() > 0.0
            && self.maximum_refinement_levels > 0
            && self.region_seeds.iter().all(|seed| {
                seed.maximum_element_volume_m3
                    .map(|value| value.is_finite() && value > 0.0)
                    .unwrap_or(true)
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VolumeMeshingIssue {
    pub severity: MeshIssueSeverity,
    pub code: String,
    pub message: String,
    pub entity_id: Option<Uuid>,
    pub measured_value: Option<f64>,
    pub required_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundaryValidationStatistics {
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub facet_count: usize,
    pub unique_edge_count: usize,
    pub connected_component_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundaryValidationReport {
    pub schema_version: String,
    pub statistics: BoundaryValidationStatistics,
    pub issues: Vec<VolumeMeshingIssue>,
}

impl BoundaryValidationReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|issue| issue.severity == MeshIssueSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TetrahedronQuality {
    pub element_id: Uuid,
    pub signed_volume_m3: f64,
    pub minimum_edge_length_m: f64,
    pub maximum_edge_length_m: f64,
    pub edge_aspect_ratio: f64,
    pub minimum_dihedral_angle_deg: f64,
    pub minimum_scaled_jacobian: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VolumeMeshStatistics {
    pub node_count: usize,
    pub tetrahedron_count: usize,
    pub boundary_facet_count: usize,
    pub steiner_point_count: usize,
    pub total_volume_m3: f64,
    pub minimum_tetrahedron_volume_m3: Option<f64>,
    pub maximum_tetrahedron_volume_m3: Option<f64>,
    pub minimum_dihedral_angle_deg: Option<f64>,
    pub minimum_scaled_jacobian: Option<f64>,
    pub maximum_edge_length_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VolumeMeshQualityReport {
    pub schema_version: String,
    pub generator_id: String,
    pub generator_version: String,
    pub statistics: VolumeMeshStatistics,
    pub tetrahedra: Vec<TetrahedronQuality>,
    pub issues: Vec<VolumeMeshingIssue>,
}

impl VolumeMeshQualityReport {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|issue| issue.severity == MeshIssueSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundaryFacetMapping {
    pub boundary_mesh_id: Uuid,
    pub boundary_triangle_id: Uuid,
    pub source_face_id: Option<Uuid>,
    pub generated_tetrahedron_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeMesherBackend {
    pub id: String,
    pub version: String,
    pub algorithm: String,
    pub deterministic: bool,
    pub native_library: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TetrahedralizationOutcome {
    pub mesh: AnalysisMesh,
    pub boundary_report: BoundaryValidationReport,
    pub quality_report: VolumeMeshQualityReport,
    pub boundary_mapping: Vec<BoundaryFacetMapping>,
    pub backend: VolumeMesherBackend,
}

pub trait VolumeMeshGenerator: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn algorithm(&self) -> &'static str;

    fn validate_boundary(
        &self,
        boundary: &[TriangleMesh],
        options: &TetrahedralizationOptions,
    ) -> BoundaryValidationReport;

    fn tetrahedralize(
        &self,
        geometry: &GeometryDocument,
        options: TetrahedralizationOptions,
    ) -> Result<TetrahedralizationOutcome>;

    fn assess_volume(
        &self,
        mesh: &AnalysisMesh,
        boundary_facet_count: usize,
        original_boundary_node_count: usize,
        options: &TetrahedralizationOptions,
    ) -> VolumeMeshQualityReport;
}
