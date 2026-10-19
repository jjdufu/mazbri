use super::*;

#[derive(Debug)]
pub enum UpdateTenantProductConfigResult {
    Updated(TenantProduct),
    NotFound,
    Conflict(DateTime<Utc>),
}
#[derive(Debug)]
pub enum ProductWriteError {
    InvalidProductId(ProductIdValidationError),
    DuplicateProductId(String),
    ExhaustedProductIdSpace,
    ProductIdReservationNotFound(Uuid),
    ProductIdReservationExpired(Uuid),
    ProductIdReservationUnavailable(Uuid),
    ProductIdReservationOwnedByAnotherOperator(Uuid),
    Database(sqlx::Error),
}
impl fmt::Display for ProductWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProductId(error) => error.fmt(f),
            Self::DuplicateProductId(product_id) => {
                write!(f, "product_id '{}' already exists", product_id)
            }
            Self::ExhaustedProductIdSpace => f.write_str("product_id space exhausted"),
            Self::ProductIdReservationNotFound(reservation_id) => {
                write!(f, "product_id reservation '{}' not found", reservation_id)
            }
            Self::ProductIdReservationExpired(reservation_id) => {
                write!(f, "product_id reservation '{}' expired", reservation_id)
            }
            Self::ProductIdReservationUnavailable(reservation_id) => {
                write!(
                    f,
                    "product_id reservation '{}' is no longer available",
                    reservation_id
                )
            }
            Self::ProductIdReservationOwnedByAnotherOperator(reservation_id) => {
                write!(
                    f,
                    "product_id reservation '{}' belongs to another operator",
                    reservation_id
                )
            }
            Self::Database(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ProductWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidProductId(error) => Some(error),
            Self::DuplicateProductId(_) => None,
            Self::ExhaustedProductIdSpace => None,
            Self::ProductIdReservationNotFound(_) => None,
            Self::ProductIdReservationExpired(_) => None,
            Self::ProductIdReservationUnavailable(_) => None,
            Self::ProductIdReservationOwnedByAnotherOperator(_) => None,
            Self::Database(error) => Some(error),
        }
    }
}
pub(crate) const PRODUCT_ID_LOCK_KEY: i64 = 8_102_345_678;
pub(crate) const MIN_PRODUCT_ID_NUMBER: i64 = 8_000_000_001;
pub(crate) const MAX_PRODUCT_ID_NUMBER: i64 = 8_999_999_999;
pub(crate) const PRODUCT_ID_RESERVATION_TTL_SECS: i64 = 300;
#[derive(Debug)]
pub enum ProductDeleteError {
    NotFound,
    HasActiveTenants(Vec<String>),
    Database(sqlx::Error),
}
impl fmt::Display for ProductDeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("Product not found"),
            Self::HasActiveTenants(tenants) => {
                write!(
                    f,
                    "This channel is bound by {} / 此通道被 {} 绑定，请先解绑再删除",
                    tenants.join(", "),
                    tenants.join(", ")
                )
            }
            Self::Database(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ProductDeleteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            _ => None,
        }
    }
}
pub struct ProductService {
    pub(crate) db: PgPool,
    pub(crate) config: ProductConfig,
    pub(crate) tenant_product_detail_cache: Cache<String, Option<TenantProductDetail>>,
    pub(crate) permission_cache: Arc<dyn PermissionCache>,
}
