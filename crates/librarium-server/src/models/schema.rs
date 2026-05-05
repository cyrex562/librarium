use serde::{Deserialize, Serialize};

// ──────────────────────────────────────────────────────────────────────────────
// Field types
// ──────────────────────────────────────────────────────────────────────────────

/// The primitive type of a schema field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Single-line text
    String,
    /// Multi-line text (not the prose zone)
    Text,
    /// Integer or float
    Number,
    /// ISO date string (YYYY-MM-DD)
    Date,
    /// True / false toggle
    Boolean,
    /// Fixed set of string values — values list stored separately in FieldSchema
    Enum,
    /// Typed wiki-link (`[[Title]]`) resolved to another entity
    EntityRef,
    /// Repeating list of any other type
    #[serde(rename = "list")]
    List,
}

impl Default for FieldType {
    fn default() -> Self {
        FieldType::String
    }
}

/// Definition of a single field in an entity type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    /// Frontmatter key (e.g. `"full_name"`)
    pub key: String,
    /// Human-readable display label
    pub label: String,
    #[serde(rename = "type", default)]
    pub field_type: FieldType,
    /// Whether the field must have a non-empty value before save
    #[serde(default)]
    pub required: bool,
    /// For `List` fields: the type of each item
    pub item_type: Option<FieldType>,
    /// For `Enum` and `List<Enum>`: the allowed values
    #[serde(default)]
    pub values: Vec<String>,
    /// Default value (JSON-compatible)
    pub default: Option<serde_json::Value>,
    /// For `EntityRef` fields: the label that target entities must carry
    pub target_label: Option<String>,
    /// For `EntityRef` fields: the relation type to auto-create on save
    pub relation: Option<String>,
    /// Optional description shown in the UI
    pub description: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Entity type schema (parsed from TOML)
// ──────────────────────────────────────────────────────────────────────────────

/// Top-level wrapper used when deserialising an entity-type TOML file.
/// The file must have an `[entity_type]` section.
#[derive(Debug, Clone, Deserialize)]
pub struct EntityTypeToml {
    pub entity_type: EntityTypeBody,
}

/// The `[entity_type]` body.
#[derive(Debug, Clone, Deserialize)]
pub struct EntityTypeBody {
    /// Display name (e.g. `"Character"`)
    pub name: String,
    /// Material-icon name (e.g. `"person"`)
    pub icon: Option<String>,
    /// Hex colour used in graph nodes (e.g. `"#4A90D9"`)
    pub color: Option<String>,
    /// Relative path to the template markdown file inside the plugin dir
    pub template: Option<String>,
    /// Labels applied to every instance of this entity type
    #[serde(default)]
    pub labels: Vec<String>,
    /// Field key used as the human-readable display name (e.g. `"full_name"`)
    pub display_field: Option<String>,
    /// Field keys shown in the "New Entity" creation dialog
    /// (defaults to all required fields if absent)
    #[serde(default)]
    pub show_on_create: Vec<String>,
    /// Field definitions (TOML array-of-tables)
    #[serde(default)]
    pub fields: Vec<FieldSchema>,
}

/// The fully-resolved entity type schema held in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityTypeSchema {
    /// Unique identifier derived from the TOML file stem (e.g. `"character"`)
    pub id: String,
    /// Plugin that registered this type
    pub plugin_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// Path to the template file relative to the plugin directory
    pub template: Option<String>,
    /// Labels that every entity of this type carries
    pub labels: Vec<String>,
    /// Field key whose value is used as the entity's display name
    pub display_field: Option<String>,
    /// Field keys shown in the "New Entity" creation dialog
    pub show_on_create: Vec<String>,
    pub fields: Vec<FieldSchema>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Relation type schema (parsed from TOML)
// ──────────────────────────────────────────────────────────────────────────────

/// Top-level wrapper for relation-type TOML files.
#[derive(Debug, Clone, Deserialize)]
pub struct RelationTypeToml {
    pub relation_type: RelationTypeBody,
}

/// The `[relation_type]` body.
#[derive(Debug, Clone, Deserialize)]
pub struct RelationTypeBody {
    /// Canonical name / id (e.g. `"member_of"`)
    pub name: String,
    /// Human-readable forward label (e.g. `"Member Of"`)
    pub label: String,
    /// Label that source entities must carry (empty = any)
    pub from_label: Option<String>,
    /// Label that target entities must carry (empty = any)
    pub to_label: Option<String>,
    #[serde(default = "default_true")]
    pub directed: bool,
    /// Label for the auto-generated inverse edge (e.g. `"has_member"`)
    pub inverse_label: Option<String>,
    /// Hex colour for graph edges
    pub color: Option<String>,
    /// Extra fields that can be set on a relation edge
    #[serde(default)]
    pub metadata_fields: Vec<FieldSchema>,
}

fn default_true() -> bool {
    true
}

/// The fully-resolved relation type schema held in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationTypeSchema {
    /// Canonical id — same as `name` in the TOML (e.g. `"member_of"`)
    pub id: String,
    pub plugin_id: String,
    pub name: String,
    pub label: String,
    pub from_label: Option<String>,
    pub to_label: Option<String>,
    pub directed: bool,
    pub inverse_label: Option<String>,
    pub color: Option<String>,
    pub metadata_fields: Vec<FieldSchema>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Plugin label declaration (inline in manifest.json)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLabelDeclaration {
    pub name: String,
    pub description: Option<String>,
}
