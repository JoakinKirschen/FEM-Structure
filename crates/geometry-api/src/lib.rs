//! Vendor-neutral geometry contracts.
//!
//! The structural domain depends on stable engineering entities, not on a CAD
//! vendor's in-memory classes. IFC, STEP and native CAD adapters can translate
//! into this boundary in later passes.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use structural_units::Length;
use uuid::Uuid;

pub const GEOMETRY_SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point3 {
    pub xyz_m: [Length; 3],
}

impl Point3 {
    pub fn from_metres(x: f64, y: f64, z: f64) -> Self {
        Self {
            xyz_m: [
                Length::from_metres(x),
                Length::from_metres(y),
                Length::from_metres(z),
            ],
        }
    }

    pub fn is_finite(self) -> bool {
        self.xyz_m.iter().all(|value| value.is_finite())
    }

    pub fn distance_to(self, other: Self) -> Length {
        let dx = self.xyz_m[0].metres() - other.xyz_m[0].metres();
        let dy = self.xyz_m[1].metres() - other.xyz_m[1].metres();
        let dz = self.xyz_m[2].metres() - other.xyz_m[2].metres();
        Length::from_metres((dx * dx + dy * dy + dz * dz).sqrt())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GeometryTolerance {
    /// Maximum distance for coincidence tests and topology healing.
    pub linear_m: Length,
    /// Maximum angular deviation in radians.
    pub angular_rad: f64,
    /// Dimensionless comparison tolerance for scaled quantities.
    pub relative: f64,
}

impl GeometryTolerance {
    pub fn engineering_default() -> Self {
        Self {
            linear_m: Length::from_metres(1.0e-6),
            angular_rad: 1.0e-9,
            relative: 1.0e-12,
        }
    }

    pub fn is_valid(self) -> bool {
        self.linear_m.is_finite()
            && self.linear_m.metres() > 0.0
            && self.angular_rad.is_finite()
            && self.angular_rad > 0.0
            && self.relative.is_finite()
            && self.relative > 0.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeometryProvenance {
    pub source_format: String,
    pub source_document: Option<String>,
    pub source_entity_id: Option<String>,
    pub source_revision: Option<String>,
    pub source_units: Option<String>,
    pub adapter_id: String,
    pub adapter_version: String,
    pub source_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurveKind {
    Line,
    Circle,
    Ellipse,
    BSpline,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
    BSpline,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Vertex {
    pub id: Uuid,
    pub point: Point3,
    pub tolerance_m: Length,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    pub id: Uuid,
    pub start_vertex_id: Uuid,
    pub end_vertex_id: Uuid,
    pub curve_kind: CurveKind,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrientedEdge {
    pub edge_id: Uuid,
    pub reversed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Wire {
    pub id: Uuid,
    pub edges: Vec<OrientedEdge>,
    pub closed: bool,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Face {
    pub id: Uuid,
    /// First wire is the outer boundary; subsequent wires are openings.
    pub wire_ids: Vec<Uuid>,
    pub surface_kind: SurfaceKind,
    pub orientation_reversed: bool,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Shell {
    pub id: Uuid,
    pub face_ids: Vec<Uuid>,
    pub closed: bool,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Body {
    pub id: Uuid,
    pub name: Option<String>,
    pub shell_ids: Vec<Uuid>,
    pub is_solid: bool,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrepModel {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
    pub wires: Vec<Wire>,
    pub faces: Vec<Face>,
    pub shells: Vec<Shell>,
    pub bodies: Vec<Body>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshVertex {
    pub id: Uuid,
    pub point: Point3,
    pub source_vertex_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Triangle {
    pub id: Uuid,
    pub vertex_ids: [Uuid; 3],
    pub source_face_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriangleMesh {
    pub id: Uuid,
    pub vertices: Vec<MeshVertex>,
    pub triangles: Vec<Triangle>,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeometryDocument {
    pub schema_version: String,
    pub document_id: Uuid,
    pub revision_id: Uuid,
    pub name: String,
    pub tolerance: GeometryTolerance,
    pub brep: BrepModel,
    pub meshes: Vec<TriangleMesh>,
    pub provenance: Option<GeometryProvenance>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GeometryIssueSeverity {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeometryIssue {
    pub severity: GeometryIssueSeverity,
    pub code: String,
    pub message: String,
    pub entity_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopologyStatistics {
    pub vertices: usize,
    pub edges: usize,
    pub wires: usize,
    pub faces: usize,
    pub shells: usize,
    pub bodies: usize,
    pub meshes: usize,
    pub triangles: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeometryValidationReport {
    pub kernel_id: String,
    pub kernel_version: String,
    pub tolerance: GeometryTolerance,
    pub statistics: TopologyStatistics,
    pub issues: Vec<GeometryIssue>,
}

impl GeometryValidationReport {
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == GeometryIssueSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealingActionKind {
    MergeCoincidentVertices,
    RewriteEdgeEndpoint,
    RemoveDegenerateEdge,
    RebuildWire,
    SewShell,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealingAction {
    pub kind: HealingActionKind,
    pub entity_id: Option<Uuid>,
    pub message: String,
    /// Geometric movement caused by the action. Zero for reference rewrites.
    pub displacement_m: Length,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealingReport {
    pub kernel_id: String,
    pub kernel_version: String,
    pub tolerance: GeometryTolerance,
    pub actions: Vec<HealingAction>,
    pub unresolved_issues: Vec<GeometryIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealingOutcome {
    pub document: GeometryDocument,
    pub report: HealingReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct TessellationOptions {
    pub chord_tolerance_m: Length,
    pub angular_tolerance_rad: f64,
    pub maximum_edge_length_m: Option<Length>,
    pub deterministic: bool,
}

impl TessellationOptions {
    pub fn engineering_default() -> Self {
        Self {
            chord_tolerance_m: Length::from_millimetres(1.0),
            angular_tolerance_rad: 0.1,
            maximum_edge_length_m: None,
            deterministic: true,
        }
    }

    pub fn is_valid(self) -> bool {
        self.chord_tolerance_m.is_finite()
            && self.chord_tolerance_m.metres() > 0.0
            && self.angular_tolerance_rad.is_finite()
            && self.angular_tolerance_rad > 0.0
            && self
                .maximum_edge_length_m
                .map(|value| value.is_finite() && value.metres() > 0.0)
                .unwrap_or(true)
    }
}

/// Boundary implemented by an in-process kernel, FFI adapter, or isolated service.
/// Stable UUID-based contracts prevent vendor handles from leaking into the domain.
pub trait GeometryKernel: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn validate(&self, document: &GeometryDocument) -> GeometryValidationReport;

    fn heal(
        &self,
        document: &GeometryDocument,
        tolerance: GeometryTolerance,
    ) -> Result<HealingOutcome>;

    fn tessellate(
        &self,
        document: &GeometryDocument,
        options: TessellationOptions,
    ) -> Result<Vec<TriangleMesh>>;
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportDiagnosticSeverity {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportDiagnostic {
    pub severity: ImportDiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub source_entity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeometryImportOptions {
    pub tolerance: GeometryTolerance,
    /// Reject the import if an unsupported geometry representation is encountered.
    pub fail_on_unsupported_geometry: bool,
    /// Preserve normalized source statements in the mapping evidence.
    pub preserve_source_records: bool,
}

impl Default for GeometryImportOptions {
    fn default() -> Self {
        Self {
            tolerance: GeometryTolerance::engineering_default(),
            fail_on_unsupported_geometry: false,
            preserve_source_records: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceEntityMapping {
    pub source_entity_id: String,
    pub source_entity_type: String,
    pub source_global_id: Option<String>,
    pub source_name: Option<String>,
    pub target_entity_ids: Vec<Uuid>,
    pub source_record_sha256: String,
    pub normalized_source_record: Option<String>,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeometryImportReport {
    pub adapter_id: String,
    pub adapter_version: String,
    pub source_format: String,
    pub source_schema: Option<String>,
    pub source_sha256: String,
    pub source_length_unit: Option<String>,
    pub source_length_scale_to_m: f64,
    pub parsed_entity_count: usize,
    pub mapped_entity_count: usize,
    pub unsupported_entity_types: Vec<String>,
    pub diagnostics: Vec<ImportDiagnostic>,
    pub mappings: Vec<SourceEntityMapping>,
}

impl GeometryImportReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity == ImportDiagnosticSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeometryImportOutcome {
    pub document: GeometryDocument,
    pub report: GeometryImportReport,
}

/// Versioned import boundary shared by IFC, STEP and future native adapters.
pub trait GeometryImportAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn source_format(&self) -> &'static str;

    fn import(
        &self,
        bytes: &[u8],
        source_name: Option<&str>,
        options: GeometryImportOptions,
    ) -> Result<GeometryImportOutcome>;
}
