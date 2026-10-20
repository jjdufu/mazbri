//! Unified List Query contract (v1).
//! Docs: `wfdh/前端列表搜索与结构化筛选-改造方案.md`
//!
//! Internal model is always `ListFilter[]`. HTTP accepts JSON `filters` and/or
//! flat `{field}_{op}` query keys. Free-text uses `q` only.
//! PostgreSQL lists: parse/validate then [`apply_pg_filters`] on a shared WHERE builder.

mod catalog;
mod error;
mod parse;
mod pg;
mod types;
mod validate;

pub use catalog::{
    FilterFieldDef, FilterValueType, ResourceFilterCatalog, admins_catalog, audit_logs_catalog,
    authorizations_catalog, bills_catalog, call_monitor_catalog, callback_deliveries_catalog,
    callback_subscriptions_catalog, certificates_catalog, channels_catalog, config_center_catalog,
    products_catalog, reconciliation_catalog, routes_catalog, suppliers_catalog, tenants_catalog,
};
pub use error::ListQueryError;
pub use parse::{ParsedListQuery, parse_filters_json, parse_flat_filters};
pub use pg::{apply_pg_filters, parse_and_validate_filters, parse_datetime_literal};
pub use types::{FilterOp, FilterValue, ListFilter, ListQuery};
pub use validate::{ValidatedFilter, validate_filters};

pub const DEFAULT_MAX_FILTERS: usize = 16;
