use super::types::FilterOp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterValueType {
    String,
    Enum,
    Bool,
    Int,
    Uint,
    Decimal,
    Datetime,
    Uuid,
}

#[derive(Debug, Clone)]
pub struct FilterFieldDef {
    pub field: &'static str,
    pub value_type: FilterValueType,
    pub ops: &'static [FilterOp],
    pub in_q: bool,
    pub requires_time_window: bool,
    /// Backend SQL/CH expression (identifier or safe expression).
    pub sql_expr: &'static str,
}

#[derive(Debug, Clone)]
pub struct ResourceFilterCatalog {
    pub resource: &'static str,
    pub fields: &'static [FilterFieldDef],
    pub max_filters: usize,
}

impl ResourceFilterCatalog {
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&FilterFieldDef> {
        self.fields.iter().find(|f| f.field == name)
    }

    pub fn q_fields(&self) -> impl Iterator<Item = &FilterFieldDef> {
        self.fields.iter().filter(|f| f.in_q)
    }

    #[must_use]
    pub fn field_names(&self) -> Vec<&'static str> {
        self.fields.iter().map(|f| f.field).collect()
    }
}

const DATETIME_OPS: &[FilterOp] = &[
    FilterOp::Eq,
    FilterOp::Gt,
    FilterOp::Gte,
    FilterOp::Lt,
    FilterOp::Lte,
    FilterOp::Between,
];
const STRING_OPS: &[FilterOp] = &[
    FilterOp::Eq,
    FilterOp::Contains,
    FilterOp::StartsWith,
    FilterOp::EndsWith,
    FilterOp::In,
];
const ENUM_OPS: &[FilterOp] = &[FilterOp::Eq, FilterOp::In, FilterOp::Neq];
const NUMBER_OPS: &[FilterOp] = &[
    FilterOp::Eq,
    FilterOp::Neq,
    FilterOp::Gt,
    FilterOp::Gte,
    FilterOp::Lt,
    FilterOp::Lte,
    FilterOp::Between,
    FilterOp::In,
];
const UUID_OPS: &[FilterOp] = &[FilterOp::Eq, FilterOp::In];

#[must_use]
pub fn call_monitor_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "call_monitor",
        max_filters: 16,
        fields: CALL_MONITOR_FIELDS,
    }
}

const CALL_MONITOR_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "timestamp",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "timestamp",
    },
    FilterFieldDef {
        field: "request_id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "request_id",
    },
    FilterFieldDef {
        field: "trace_id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "trace_id",
    },
    FilterFieldDef {
        field: "tenant_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "tenant_code",
    },
    FilterFieldDef {
        field: "tenant_id",
        value_type: FilterValueType::Uuid,
        ops: UUID_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "tenant_id",
    },
    FilterFieldDef {
        field: "product_id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "product_id",
    },
    FilterFieldDef {
        field: "supplier_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "supplier_code",
    },
    FilterFieldDef {
        field: "channel_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "channel_code",
    },
    FilterFieldDef {
        field: "path",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Contains, FilterOp::Eq],
        in_q: true,
        requires_time_window: false,
        sql_expr: "path",
    },
    FilterFieldDef {
        field: "client_ip",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "client_ip",
    },
    FilterFieldDef {
        field: "upstream_host",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "upstream_host",
    },
    FilterFieldDef {
        field: "error_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "error_code",
    },
    FilterFieldDef {
        field: "status_code",
        value_type: FilterValueType::Uint,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "status_code",
    },
    FilterFieldDef {
        field: "upstream_status_code",
        value_type: FilterValueType::Uint,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "upstream_status_code",
    },
    FilterFieldDef {
        field: "duration_ms",
        value_type: FilterValueType::Uint,
        ops: &[
            FilterOp::Eq,
            FilterOp::Gt,
            FilterOp::Gte,
            FilterOp::Lt,
            FilterOp::Lte,
            FilterOp::Between,
        ],
        in_q: false,
        requires_time_window: false,
        sql_expr: "duration_ms",
    },
    FilterFieldDef {
        field: "upstream_service_time_ms",
        value_type: FilterValueType::Uint,
        ops: &[
            FilterOp::Eq,
            FilterOp::Gt,
            FilterOp::Gte,
            FilterOp::Lt,
            FilterOp::Lte,
            FilterOp::Between,
        ],
        in_q: false,
        requires_time_window: false,
        sql_expr: "upstream_service_time_ms",
    },
];

#[must_use]
pub fn tenants_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "tenants",
        max_filters: 16,
        fields: TENANTS_FIELDS,
    }
}

const TENANTS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "name",
    },
    FilterFieldDef {
        field: "tenant_id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "tenant_id",
    },
    FilterFieldDef {
        field: "tenant_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "tenant_code",
    },
    FilterFieldDef {
        field: "contact_email",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "contact_email",
    },
    FilterFieldDef {
        field: "contact_person",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "contact_person",
    },
    FilterFieldDef {
        field: "contact_phone",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "contact_phone",
    },
    FilterFieldDef {
        field: "enabled",
        value_type: FilterValueType::Bool,
        ops: &[FilterOp::Eq],
        in_q: false,
        requires_time_window: false,
        sql_expr: "enabled",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "created_at",
    },
];

#[must_use]
pub fn certificates_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "certificates",
        max_filters: 16,
        fields: CERT_FIELDS,
    }
}

const CERT_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "subject",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "subject",
    },
    FilterFieldDef {
        field: "fingerprint",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "encode(fingerprint, 'hex')",
    },
    FilterFieldDef {
        field: "dns_names",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Contains, FilterOp::Eq],
        in_q: true,
        requires_time_window: false,
        // text cast of text[] for ILIKE
        sql_expr: "array_to_string(dns_names, ',')",
    },
    FilterFieldDef {
        field: "expires_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "expires_at",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        // Derived in SQL: revoked / expired / expiring_soon / valid
        sql_expr: "CASE WHEN revoked THEN 'revoked' WHEN expires_at <= NOW() THEN 'expired' WHEN expires_at <= NOW() + INTERVAL '30 days' THEN 'expiring_soon' ELSE 'valid' END",
    },
];

#[must_use]
pub fn suppliers_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "suppliers",
        max_filters: 16,
        fields: SUPPLIERS_FIELDS,
    }
}

const SUPPLIERS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "name",
    },
    FilterFieldDef {
        field: "supplier_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "supplier_code",
    },
    FilterFieldDef {
        field: "upstream_url",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "upstream_url",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "status",
    },
];

// --- Channels ---
#[must_use]
pub fn channels_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "channels",
        max_filters: 16,
        fields: CHANNELS_FIELDS,
    }
}

const CHANNELS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "c.name",
    },
    FilterFieldDef {
        field: "channel_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "c.channel_code",
    },
    FilterFieldDef {
        field: "supplier_code",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "s.supplier_code",
    },
    FilterFieldDef {
        field: "supplier_name",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "s.name",
    },
    FilterFieldDef {
        field: "path_prefix",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "c.path_prefix",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "c.status",
    },
    FilterFieldDef {
        field: "qps_limit",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "c.qps_limit",
    },
];

// --- Products ---
#[must_use]
pub fn products_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "products",
        max_filters: 16,
        fields: PRODUCTS_FIELDS,
    }
}

const PRODUCTS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "product_id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "product_id",
    },
    FilterFieldDef {
        field: "name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "name",
    },
    FilterFieldDef {
        field: "enabled",
        value_type: FilterValueType::Bool,
        ops: &[FilterOp::Eq],
        in_q: false,
        requires_time_window: false,
        sql_expr: "enabled",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "created_at",
    },
];

// --- Authorizations (tenant_products) ---
#[must_use]
pub fn authorizations_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "authorizations",
        max_filters: 16,
        fields: AUTHORIZATIONS_FIELDS,
    }
}

const AUTHORIZATIONS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "tenant_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "t.tenant_code",
    },
    FilterFieldDef {
        field: "tenant_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "t.name",
    },
    FilterFieldDef {
        field: "product_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "p.product_id",
    },
    FilterFieldDef {
        field: "product_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "p.name",
    },
    FilterFieldDef {
        field: "unit_price",
        value_type: FilterValueType::Decimal,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "COALESCE((tp.custom_config->>'unit_price')::numeric, 0.001)",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "tp.created_at",
    },
];

// --- Admins / users ---
#[must_use]
pub fn admins_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "admins",
        max_filters: 16,
        fields: ADMINS_FIELDS,
    }
}

const ADMINS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "username",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "username",
    },
    FilterFieldDef {
        field: "display_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "display_name",
    },
    FilterFieldDef {
        field: "email",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "email",
    },
    FilterFieldDef {
        field: "role",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "role",
    },
    // UI column is "status"; DB is is_active. Map enum active/inactive → bool via CASE.
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "CASE WHEN is_active THEN 'active' ELSE 'inactive' END",
    },
    FilterFieldDef {
        field: "mfa_enabled",
        value_type: FilterValueType::Bool,
        ops: &[FilterOp::Eq],
        in_q: false,
        requires_time_window: false,
        sql_expr: "mfa_enabled",
    },
    FilterFieldDef {
        field: "last_login_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "last_login_at",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "created_at",
    },
];

// --- Bills ---
#[must_use]
pub fn bills_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "bills",
        max_filters: 16,
        fields: BILLS_FIELDS,
    }
}

const BILLS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "b.id::text",
    },
    FilterFieldDef {
        field: "tenant_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "b.party_code",
    },
    FilterFieldDef {
        field: "tenant_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "b.party_name",
    },
    FilterFieldDef {
        field: "bill_kind",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "b.bill_kind",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "b.status",
    },
    FilterFieldDef {
        field: "total",
        value_type: FilterValueType::Decimal,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "b.total_amount",
    },
    FilterFieldDef {
        field: "period_start",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "b.billing_period_start",
    },
    FilterFieldDef {
        field: "period_end",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "b.billing_period_end",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "b.created_at",
    },
];

// --- Usage reconciliation discrepancies ---
#[must_use]
pub fn reconciliation_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "usage_reconciliation",
        max_filters: 16,
        fields: RECONCILIATION_FIELDS,
    }
}

const RECONCILIATION_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "id",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "id::text",
    },
    FilterFieldDef {
        field: "reconciliation_date",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: true,
        requires_time_window: false,
        // date column compared as start-of-day timestamptz (UTC)
        sql_expr: "(reconciliation_date::timestamp AT TIME ZONE 'UTC')",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "status",
    },
    FilterFieldDef {
        field: "scope_type",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "scope_type",
    },
    FilterFieldDef {
        field: "scope_id",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "scope_id",
    },
    FilterFieldDef {
        field: "billable_call_count",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "billable_call_count",
    },
    FilterFieldDef {
        field: "gateway_call_count",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "gateway_call_count",
    },
    FilterFieldDef {
        field: "upstream_preagg_call_count",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "upstream_preagg_call_count",
    },
    FilterFieldDef {
        field: "discrepancy_ratio",
        value_type: FilterValueType::Decimal,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "discrepancy_ratio",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "created_at",
    },
];

// --- Callback subscriptions ---
#[must_use]
pub fn callback_subscriptions_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "callback_subscriptions",
        max_filters: 16,
        fields: CALLBACK_SUBSCRIPTIONS_FIELDS,
    }
}

const CALLBACK_SUBSCRIPTIONS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "tenant_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "t.tenant_code",
    },
    FilterFieldDef {
        field: "tenant_id",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "t.tenant_id",
    },
    FilterFieldDef {
        field: "product_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "p.product_id",
    },
    FilterFieldDef {
        field: "callback_url",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "s.callback_url",
    },
    FilterFieldDef {
        field: "is_active",
        value_type: FilterValueType::Bool,
        ops: &[FilterOp::Eq],
        in_q: false,
        requires_time_window: false,
        sql_expr: "s.is_active",
    },
];

// --- Callback deliveries ---
#[must_use]
pub fn callback_deliveries_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "callback_deliveries",
        max_filters: 16,
        fields: CALLBACK_DELIVERIES_FIELDS,
    }
}

const CALLBACK_DELIVERIES_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "event_id",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "d.event_id",
    },
    FilterFieldDef {
        field: "callback_url",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "s.callback_url",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "d.status",
    },
    FilterFieldDef {
        field: "last_error",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Contains, FilterOp::Eq],
        in_q: true,
        requires_time_window: false,
        sql_expr: "COALESCE(d.last_error, '')",
    },
    FilterFieldDef {
        field: "last_response_code",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "d.last_response_code",
    },
    FilterFieldDef {
        field: "retry_count",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "d.retry_count",
    },
];

// --- Route rules ---
#[must_use]
pub fn routes_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "routes",
        max_filters: 16,
        fields: ROUTES_FIELDS,
    }
}

const ROUTES_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "tenant_id",
        value_type: FilterValueType::Uuid,
        ops: UUID_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "tenant_id",
    },
    FilterFieldDef {
        field: "product_id",
        value_type: FilterValueType::Uuid,
        ops: UUID_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "product_id",
    },
    FilterFieldDef {
        field: "tenant_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "metadata #>> '{route_config,tenant_code}'",
    },
    FilterFieldDef {
        field: "tenant_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "metadata #>> '{route_config,tenant_name}'",
    },
    FilterFieldDef {
        field: "product_code",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "metadata #>> '{route_config,product_code}'",
    },
    FilterFieldDef {
        field: "product_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "metadata #>> '{route_config,product_name}'",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "status",
    },
    FilterFieldDef {
        field: "priority",
        value_type: FilterValueType::Int,
        ops: NUMBER_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "priority",
    },
    FilterFieldDef {
        field: "created_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "created_at",
    },
];

// --- Config center ---
#[must_use]
pub fn config_center_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "config_center",
        max_filters: 16,
        fields: CONFIG_CENTER_FIELDS,
    }
}

const CONFIG_CENTER_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "service_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "service_name",
    },
    FilterFieldDef {
        field: "updated_by",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "updated_by",
    },
    FilterFieldDef {
        field: "updated_at",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "updated_at",
    },
];

// --- Audit logs ---
#[must_use]
pub fn audit_logs_catalog() -> ResourceFilterCatalog {
    ResourceFilterCatalog {
        resource: "audit_logs",
        max_filters: 16,
        fields: AUDIT_LOGS_FIELDS,
    }
}

const AUDIT_LOGS_FIELDS: &[FilterFieldDef] = &[
    FilterFieldDef {
        field: "actor_name",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "username",
    },
    FilterFieldDef {
        field: "action",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "action",
    },
    FilterFieldDef {
        field: "resource_type",
        value_type: FilterValueType::String,
        ops: STRING_OPS,
        in_q: true,
        requires_time_window: false,
        sql_expr: "resource_type",
    },
    FilterFieldDef {
        field: "status",
        value_type: FilterValueType::Enum,
        ops: ENUM_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "status",
    },
    FilterFieldDef {
        field: "ip_address",
        value_type: FilterValueType::String,
        ops: &[FilterOp::Eq, FilterOp::Contains],
        in_q: true,
        requires_time_window: false,
        sql_expr: "COALESCE(ip_address, details->>'ip_address', details->>'client_ip', details->>'actor_ip')",
    },
    FilterFieldDef {
        field: "timestamp",
        value_type: FilterValueType::Datetime,
        ops: DATETIME_OPS,
        in_q: false,
        requires_time_window: false,
        sql_expr: "created_at",
    },
];
