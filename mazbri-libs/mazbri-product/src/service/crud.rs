use super::*;

impl ProductService {
    pub async fn create_product(
        &self,
        req: CreateProductRequest,
        operator: &str,
    ) -> std::result::Result<Product, ProductWriteError> {
        let mut tx = self.db.begin().await.map_err(ProductWriteError::Database)?;
        let reservation = self
            .consume_product_id_reservation(&mut tx, req.reservation_id, operator)
            .await?;
        let row = self
            .insert_product_in_tx(&mut tx, &reservation.product_id, &req)
            .await?;
        self.mark_reservation_consumed(&mut tx, reservation.id, row.get("id"))
            .await?;
        tx.commit().await.map_err(ProductWriteError::Database)?;
        info!(
            "Created product from reservation: {} ({})",
            reservation.product_id, reservation.id
        );
        Ok(product_from_row(&row))
    }
    pub async fn reserve_product_id(
        &self,
        reserved_by: &str,
    ) -> std::result::Result<ProductIdReservation, ProductWriteError> {
        let mut tx = self.db.begin().await.map_err(ProductWriteError::Database)?;
        self.lock_product_id_space(&mut tx).await?;
        self.expire_stale_product_id_reservations(&mut tx).await?;
        let reservation = match self
            .reserve_available_product_id(&mut tx, reserved_by)
            .await?
        {
            Some(reservation) => reservation,
            None => {
                let product_id = self.allocate_next_product_id(&mut tx).await?;
                self.insert_product_id_reservation(&mut tx, &product_id, reserved_by)
                    .await?
            }
        };
        tx.commit().await.map_err(ProductWriteError::Database)?;
        Ok(reservation)
    }
    pub async fn release_product_id_reservation(
        &self,
        reservation_id: Uuid,
        reserved_by: &str,
    ) -> std::result::Result<bool, ProductWriteError> {
        let result = sqlx::query(
            r#"
            UPDATE product_id_reservations
            SET status = 'available',
                active_reservation_id = NULL,
                reserved_by = NULL,
                reserved_at = NULL,
                expires_at = NULL,
                released_at = NOW()
            WHERE active_reservation_id = $1
              AND reserved_by = $2
              AND status = 'reserved'
              AND consumed_at IS NULL
            RETURNING id
            "#,
        )
        .bind(reservation_id)
        .bind(reserved_by)
        .fetch_optional(&self.db)
        .await
        .map_err(ProductWriteError::Database)?;
        Ok(result.is_some())
    }
    pub async fn update_product(
        &self,
        product_id: &str,
        req: UpdateProductRequest,
    ) -> std::result::Result<Option<Product>, ProductWriteError> {
        validate_product_id(product_id).map_err(ProductWriteError::InvalidProductId)?;
        let now = Utc::now();
        let has_updates = req.name.is_some()
            || req.description.is_some()
            || req.enabled.is_some()
            || req.timeout_secs.is_some()
            || req.cb_pro_enabled.is_some()
            || req.cb_failure_threshold.is_some()
            || req.cb_success_threshold.is_some()
            || req.cb_cooldown_secs.is_some();
        if !has_updates {
            return self
                .fetch_product_by_id(product_id)
                .await
                .map_err(ProductWriteError::Database);
        }
        let mut query_builder: QueryBuilder<_> =
            QueryBuilder::new("UPDATE products SET updated_at = ");
        query_builder.push_bind(now);
        if let Some(ref v) = req.name {
            query_builder.push(", name = ");
            query_builder.push_bind(v);
        }
        if let Some(ref v) = req.description {
            query_builder.push(", description = ");
            query_builder.push_bind(v);
        }
        if let Some(v) = req.timeout_secs {
            query_builder.push(", timeout_secs = ");
            query_builder.push_bind(v);
        }
        if let Some(v) = req.enabled {
            query_builder.push(", enabled = ");
            query_builder.push_bind(v);
        }
        if let Some(v) = req.cb_pro_enabled {
            query_builder.push(", cb_pro_enabled = ");
            query_builder.push_bind(v);
        }
        if let Some(v) = req.cb_failure_threshold {
            query_builder.push(", cb_failure_threshold = ");
            query_builder.push_bind(v);
        }
        if let Some(v) = req.cb_success_threshold {
            query_builder.push(", cb_success_threshold = ");
            query_builder.push_bind(v);
        }
        if let Some(v) = req.cb_cooldown_secs {
            query_builder.push(", cb_cooldown_secs = ");
            query_builder.push_bind(v);
        }
        query_builder.push(" WHERE product_id = ");
        query_builder.push_bind(product_id);
        query_builder.push(
            " RETURNING id, product_id, name, description,
                       timeout_secs, enabled, cb_pro_enabled,
                       cb_failure_threshold, cb_success_threshold, cb_cooldown_secs,
                       created_at, updated_at",
        );
        let row = query_builder
            .build()
            .fetch_optional(&self.db)
            .await
            .map_err(|error| classify_product_write_error(error, product_id))?;
        self.tenant_product_detail_cache.invalidate_all();
        if let Some(row) = row {
            info!("Updated product: {}", product_id);
            Ok(Some(product_from_row(&row)))
        } else {
            Ok(None)
        }
    }
    pub async fn get_product_by_id(&self, product_id: &str) -> Result<Option<Product>> {
        Ok(self.fetch_product_by_id(product_id).await?)
    }
    pub(crate) async fn fetch_product_by_id(
        &self,
        product_id: &str,
    ) -> std::result::Result<Option<Product>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, product_id, name, description,
                   timeout_secs, enabled, cb_pro_enabled,
                   cb_failure_threshold, cb_success_threshold, cb_cooldown_secs,
                   created_at, updated_at
            FROM products
            WHERE product_id = $1
            "#,
        )
        .bind(product_id)
        .fetch_optional(&self.db)
        .await?;
        if let Some(row) = row {
            Ok(Some(product_from_row(&row)))
        } else {
            Ok(None)
        }
    }
    pub async fn list_products(&self) -> Result<Vec<Product>> {
        let rows = sqlx::query(
            r#"
            SELECT id, product_id, name, description,
                   timeout_secs, enabled, cb_pro_enabled,
                   cb_failure_threshold, cb_success_threshold, cb_cooldown_secs,
                   created_at, updated_at
            FROM products
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.db)
        .await?;
        let products = rows.iter().map(product_from_row).collect();
        Ok(products)
    }
    /// List products with free-text `search` and optional extra AND filters.
    ///
    /// `apply_extra` is called for both COUNT and SELECT builders (trusted
    /// SQL only — caller must validate filters before applying).
    pub async fn list_products_page<F>(
        &self,
        search: Option<&str>,
        limit: i64,
        offset: i64,
        mut apply_extra: F,
    ) -> Result<(Vec<Product>, i64)>
    where
        F: FnMut(&mut QueryBuilder<'_, Postgres>) -> Result<(), String>,
    {
        let search_pattern = search.map(|value| format!("%{}%", value));

        let mut count_builder: QueryBuilder<'_, Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM products WHERE 1=1");
        if let Some(ref p) = search_pattern {
            count_builder.push(" AND (name ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(" OR product_id ILIKE ");
            count_builder.push_bind(p.clone());
            count_builder.push(")");
        }
        apply_extra(&mut count_builder).map_err(|e| anyhow::anyhow!(e))?;
        let total: i64 = count_builder.build().fetch_one(&self.db).await?.get(0);

        let mut list_builder: QueryBuilder<'_, Postgres> = QueryBuilder::new(
            "SELECT id, product_id, name, description, \
             timeout_secs, enabled, cb_pro_enabled, \
             cb_failure_threshold, cb_success_threshold, cb_cooldown_secs, \
             created_at, updated_at \
             FROM products WHERE 1=1",
        );
        if let Some(ref p) = search_pattern {
            list_builder.push(" AND (name ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(" OR product_id ILIKE ");
            list_builder.push_bind(p.clone());
            list_builder.push(")");
        }
        apply_extra(&mut list_builder).map_err(|e| anyhow::anyhow!(e))?;
        list_builder.push(" ORDER BY created_at DESC LIMIT ");
        list_builder.push_bind(limit);
        list_builder.push(" OFFSET ");
        list_builder.push_bind(offset);
        let rows = list_builder.build().fetch_all(&self.db).await?;
        Ok((rows.iter().map(product_from_row).collect(), total))
    }
    pub async fn delete_product(
        &self,
        product_id: &str,
    ) -> std::result::Result<bool, ProductDeleteError> {
        let product_row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM products WHERE product_id = $1")
                .bind(product_id)
                .fetch_optional(&self.db)
                .await
                .map_err(ProductDeleteError::Database)?;
        let product_uuid = match product_row {
            Some((uuid,)) => uuid,
            None => return Ok(false),
        };
        let active_tenants: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT t.tenant_code
            FROM tenant_products tp
            JOIN tenants t ON tp.tenant_id = t.id
            WHERE tp.product_id = $1 AND tp.enabled = true
            "#,
        )
        .bind(product_uuid)
        .fetch_all(&self.db)
        .await
        .map_err(ProductDeleteError::Database)?;
        if !active_tenants.is_empty() {
            return Err(ProductDeleteError::HasActiveTenants(active_tenants));
        }
        let result = sqlx::query(
            r#"
            DELETE FROM products
            WHERE id = $1
            RETURNING id
            "#,
        )
        .bind(product_uuid)
        .fetch_optional(&self.db)
        .await
        .map_err(ProductDeleteError::Database)?;
        self.tenant_product_detail_cache.invalidate_all();
        Ok(result.is_some())
    }
}
