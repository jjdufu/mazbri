use super::error::{ServiceError, ServiceResult};
use super::models::{CreateServiceRequest, Service, ServiceSource, UpdateServiceRequest};
use sqlx::PgPool;
use std::sync::Arc;
#[derive(Clone)]
pub struct ServiceRepository {
    db: Arc<PgPool>,
}
impl ServiceRepository {
    #[must_use]
    pub fn new(db: Arc<PgPool>) -> Self {
        Self { db }
    }
    pub async fn find_all(&self) -> ServiceResult<Vec<Service>> {
        let services = sqlx::query_as::<_, Service>(
            r#"
            SELECT id, service_name, display_name, description,
                   category, status, source, dns_names, metadata,
                   created_at, updated_at, created_by, updated_by
            FROM services
            ORDER BY service_name
            "#,
        )
        .fetch_all(self.db.as_ref())
        .await?;
        Ok(services)
    }
    pub async fn find_all_by_status(&self, status: &str) -> ServiceResult<Vec<Service>> {
        let services = sqlx::query_as::<_, Service>(
            r#"
            SELECT id, service_name, display_name, description,
                   category, status, source, dns_names, metadata,
                   created_at, updated_at, created_by, updated_by
            FROM services
            WHERE status = $1
            ORDER BY service_name
            "#,
        )
        .bind(status)
        .fetch_all(self.db.as_ref())
        .await?;
        Ok(services)
    }
    pub async fn find_by_name(&self, service_name: &str) -> ServiceResult<Service> {
        let service = sqlx::query_as::<_, Service>(
            r#"
            SELECT id, service_name, display_name, description,
                   category, status, source, dns_names, metadata,
                   created_at, updated_at, created_by, updated_by
            FROM services
            WHERE service_name = $1
            "#,
        )
        .bind(service_name)
        .fetch_optional(self.db.as_ref())
        .await?
        .ok_or_else(|| ServiceError::ServiceNotFound(service_name.to_string()))?;
        Ok(service)
    }
    pub async fn create(&self, req: &CreateServiceRequest) -> ServiceResult<Service> {
        if req.service_name.trim().is_empty() {
            return Err(ServiceError::EmptyServiceName);
        }
        if req.display_name.trim().is_empty() {
            return Err(ServiceError::EmptyDisplayName);
        }
        if let Some(service) = self.restore_retired(req).await? {
            return Ok(service);
        }
        let service = sqlx::query_as::<_, Service>(
            r#"
            INSERT INTO services (
                service_name, display_name, description, category, status,
                source, dns_names, metadata, created_by, updated_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            RETURNING id, service_name, display_name, description,
                      category, status, source, dns_names, metadata,
                      created_at, updated_at, created_by, updated_by
            "#,
        )
        .bind(&req.service_name)
        .bind(&req.display_name)
        .bind(&req.description)
        .bind(&req.category)
        .bind(&req.status)
        .bind(ServiceSource::User)
        .bind(&req.dns_names)
        .bind(&req.metadata)
        .bind(&req.created_by)
        .fetch_one(self.db.as_ref())
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key")
                || e.to_string().contains("unique constraint")
            {
                ServiceError::ServiceAlreadyExists(req.service_name.clone())
            } else {
                ServiceError::Core(e.into())
            }
        })?;
        Ok(service)
    }
    async fn restore_retired(&self, req: &CreateServiceRequest) -> ServiceResult<Option<Service>> {
        let service = sqlx::query_as::<_, Service>(
            r#"
            UPDATE services
            SET display_name = $2,
                description = $3,
                category = $4,
                status = $5,
                source = $6,
                dns_names = $7,
                metadata = $8,
                updated_by = $9,
                updated_at = NOW()
            WHERE service_name = $1
              AND status = 'retired'
            RETURNING id, service_name, display_name, description,
                      category, status, source, dns_names, metadata,
                      created_at, updated_at, created_by, updated_by
            "#,
        )
        .bind(&req.service_name)
        .bind(&req.display_name)
        .bind(&req.description)
        .bind(&req.category)
        .bind(&req.status)
        .bind(ServiceSource::User)
        .bind(&req.dns_names)
        .bind(&req.metadata)
        .bind(&req.created_by)
        .fetch_optional(self.db.as_ref())
        .await?;
        Ok(service)
    }
    pub async fn update(
        &self,
        service_name: &str,
        req: &UpdateServiceRequest,
    ) -> ServiceResult<Service> {
        let mut query = String::from("UPDATE services SET updated_by = $1");
        let mut param_index = 2;
        let mut set_clauses = Vec::new();
        if req.display_name.is_some() {
            set_clauses.push(format!("display_name = ${}", param_index));
            param_index += 1;
        }
        if req.description.is_some() {
            set_clauses.push(format!("description = ${}", param_index));
            param_index += 1;
        }
        if req.category.is_some() {
            set_clauses.push(format!("category = ${}", param_index));
            param_index += 1;
        }
        if req.status.is_some() {
            set_clauses.push(format!("status = ${}", param_index));
            param_index += 1;
        }
        if req.dns_names.is_some() {
            set_clauses.push(format!("dns_names = ${}", param_index));
            param_index += 1;
        }
        if req.metadata.is_some() {
            set_clauses.push(format!("metadata = ${}", param_index));
            param_index += 1;
        }
        if set_clauses.is_empty() {
            return self.find_by_name(service_name).await;
        }
        query.push_str(", ");
        query.push_str(&set_clauses.join(", "));
        query
            .push_str(
                &format!(
                    " WHERE service_name = ${} RETURNING id, service_name, display_name, description, category, status, source, dns_names, metadata, created_at, updated_at, created_by, updated_by",
                    param_index
                ),
            );
        let mut query_builder = sqlx::query_as::<_, Service>(&query);
        query_builder = query_builder.bind(&req.updated_by);
        if let Some(ref display_name) = req.display_name {
            query_builder = query_builder.bind(display_name);
        }
        if let Some(ref description) = req.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(ref category) = req.category {
            query_builder = query_builder.bind(category);
        }
        if let Some(ref status) = req.status {
            query_builder = query_builder.bind(status);
        }
        if let Some(ref dns_names) = req.dns_names {
            query_builder = query_builder.bind(dns_names);
        }
        if let Some(ref metadata) = req.metadata {
            query_builder = query_builder.bind(metadata);
        }
        query_builder = query_builder.bind(service_name);
        let service = query_builder
            .fetch_optional(self.db.as_ref())
            .await?
            .ok_or_else(|| ServiceError::ServiceNotFound(service_name.to_string()))?;
        Ok(service)
    }
    pub async fn soft_delete(&self, service_name: &str) -> ServiceResult<Service> {
        let service = sqlx::query_as::<_, Service>(
            r#"
            UPDATE services
            SET status = 'retired', updated_at = NOW()
            WHERE service_name = $1
            RETURNING id, service_name, display_name, description,
                      category, status, source, dns_names, metadata,
                      created_at, updated_at, created_by, updated_by
            "#,
        )
        .bind(service_name)
        .fetch_optional(self.db.as_ref())
        .await?
        .ok_or_else(|| ServiceError::ServiceNotFound(service_name.to_string()))?;
        Ok(service)
    }
    pub async fn validate_service_name(&self, service_name: &str) -> ServiceResult<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM services
                WHERE service_name = $1 AND status = 'active'
            )
            "#,
        )
        .bind(service_name)
        .fetch_one(self.db.as_ref())
        .await?;
        Ok(exists)
    }
}
