use super::{
    CreateProductRequest, DateTime, PRODUCT_ID_LOCK_KEY, PRODUCT_ID_RESERVATION_TTL_SECS,
    PermissionCache, PgRow, Postgres, ProductIdReservation, ProductService, ProductWriteError, Row,
    Transaction, Utc, Uuid, classify_product_write_error, next_product_id_from_max,
};

impl ProductService {
    pub(crate) async fn insert_product_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        product_id: &str,
        req: &CreateProductRequest,
    ) -> std::result::Result<PgRow, ProductWriteError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            r"
            INSERT INTO products (id, product_id, name, description,
                                timeout_secs, enabled, cb_pro_enabled,
                                cb_failure_threshold, cb_success_threshold, cb_cooldown_secs,
                                created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
            RETURNING id, product_id, name, description,
                      timeout_secs, enabled, cb_pro_enabled,
                      cb_failure_threshold, cb_success_threshold, cb_cooldown_secs,
                      created_at, updated_at
            ",
        )
        .bind(id)
        .bind(product_id)
        .bind(&req.name)
        .bind(&req.description)
        .bind(req.timeout_secs)
        .bind(req.enabled)
        .bind(req.cb_pro_enabled)
        .bind(req.cb_failure_threshold)
        .bind(req.cb_success_threshold)
        .bind(req.cb_cooldown_secs)
        .bind(now)
        .fetch_one(tx.as_mut())
        .await
        .map_err(|error| classify_product_write_error(error, product_id))
    }
    pub(crate) async fn lock_product_id_space(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> std::result::Result<(), ProductWriteError> {
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(PRODUCT_ID_LOCK_KEY)
            .execute(tx.as_mut())
            .await
            .map_err(ProductWriteError::Database)?;
        Ok(())
    }
    pub(crate) async fn allocate_next_product_id(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> std::result::Result<String, ProductWriteError> {
        let max_product_id = sqlx::query_scalar::<_, Option<i64>>(
            r"
            SELECT MAX(product_id_value)
            FROM (
                SELECT product_id::bigint AS product_id_value
                FROM products
                WHERE product_id ~ '^8[0-9]{9}$'
            UNION ALL
            SELECT product_id::bigint AS product_id_value
            FROM product_id_reservations
            WHERE product_id ~ '^8[0-9]{9}$'
            ) AS allocated_product_ids
            ",
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(ProductWriteError::Database)?;
        next_product_id_from_max(max_product_id)
    }
    pub(crate) async fn expire_stale_product_id_reservations(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> std::result::Result<(), ProductWriteError> {
        sqlx::query(
            r"
            UPDATE product_id_reservations
            SET status = 'available',
                active_reservation_id = NULL,
                reserved_by = NULL,
                reserved_at = NULL,
                expires_at = NULL,
                released_at = NOW()
            WHERE status = 'reserved'
              AND expires_at <= NOW()
            ",
        )
        .execute(tx.as_mut())
        .await
        .map_err(ProductWriteError::Database)?;
        Ok(())
    }
    pub(crate) async fn reserve_available_product_id(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        reserved_by: &str,
    ) -> std::result::Result<Option<ProductIdReservation>, ProductWriteError> {
        let available = sqlx::query(
            r"
            SELECT id, product_id
            FROM product_id_reservations
            WHERE status = 'available'
            ORDER BY product_id::bigint ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            ",
        )
        .fetch_optional(tx.as_mut())
        .await
        .map_err(ProductWriteError::Database)?;
        let Some(row) = available else {
            return Ok(None);
        };
        let reservation_id = Uuid::new_v4();
        let reserved_at = Utc::now();
        let expires_at = reserved_at + chrono::Duration::seconds(PRODUCT_ID_RESERVATION_TTL_SECS);
        let row = sqlx::query(
            r"
            UPDATE product_id_reservations
            SET active_reservation_id = $2,
                reserved_by = $3,
                status = 'reserved',
                reserved_at = $4,
                expires_at = $5,
                released_at = NULL
            WHERE id = $1
            RETURNING active_reservation_id, product_id, reserved_at, expires_at
            ",
        )
        .bind(row.get::<Uuid, _>("id"))
        .bind(reservation_id)
        .bind(reserved_by)
        .bind(reserved_at)
        .bind(expires_at)
        .fetch_one(tx.as_mut())
        .await
        .map_err(ProductWriteError::Database)?;
        Ok(Some(ProductIdReservation {
            id: row.get("active_reservation_id"),
            product_id: row.get("product_id"),
            reserved_at: row.get("reserved_at"),
            expires_at: row.get("expires_at"),
        }))
    }
    pub(crate) async fn insert_product_id_reservation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        product_id: &str,
        reserved_by: &str,
    ) -> std::result::Result<ProductIdReservation, ProductWriteError> {
        let row_id = Uuid::new_v4();
        let reservation_id = Uuid::new_v4();
        let reserved_at = Utc::now();
        let expires_at = reserved_at + chrono::Duration::seconds(PRODUCT_ID_RESERVATION_TTL_SECS);
        let row = sqlx::query(
            r"
            INSERT INTO product_id_reservations (
                id, product_id, active_reservation_id, reserved_by, status, reserved_at, expires_at
            )
            VALUES ($1, $2, $3, $4, 'reserved', $5, $6)
            RETURNING active_reservation_id, product_id, reserved_at, expires_at
            ",
        )
        .bind(row_id)
        .bind(product_id)
        .bind(reservation_id)
        .bind(reserved_by)
        .bind(reserved_at)
        .bind(expires_at)
        .fetch_one(tx.as_mut())
        .await
        .map_err(|error| classify_product_write_error(error, product_id))?;
        Ok(ProductIdReservation {
            id: row.get("active_reservation_id"),
            product_id: row.get("product_id"),
            reserved_at: row.get("reserved_at"),
            expires_at: row.get("expires_at"),
        })
    }
    pub(crate) async fn consume_product_id_reservation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        reservation_id: Uuid,
        operator: &str,
    ) -> std::result::Result<ProductIdReservationRow, ProductWriteError> {
        let reservation = sqlx::query(
                r"
            SELECT id, product_id, active_reservation_id, reserved_by, status, reserved_at, expires_at
            FROM product_id_reservations
            WHERE active_reservation_id = $1
            FOR UPDATE
            ",
            )
            .bind(reservation_id)
            .fetch_optional(tx.as_mut())
            .await
            .map_err(ProductWriteError::Database)?;
        let Some(row) = reservation else {
            return Err(ProductWriteError::ProductIdReservationNotFound(
                reservation_id,
            ));
        };
        let reserved_by: String = row.get("reserved_by");
        let status: String = row.get("status");
        let expires_at: DateTime<Utc> = row.get("expires_at");
        if reserved_by != operator {
            return Err(
                ProductWriteError::ProductIdReservationOwnedByAnotherOperator(reservation_id),
            );
        }
        if status != "reserved" {
            return Err(ProductWriteError::ProductIdReservationUnavailable(
                reservation_id,
            ));
        }
        if expires_at <= Utc::now() {
            sqlx::query(
                r"
                UPDATE product_id_reservations
                SET status = 'available',
                    active_reservation_id = NULL,
                    reserved_by = NULL,
                    reserved_at = NULL,
                    expires_at = NULL,
                    released_at = NOW()
                WHERE active_reservation_id = $1
                ",
            )
            .bind(reservation_id)
            .execute(tx.as_mut())
            .await
            .map_err(ProductWriteError::Database)?;
            return Err(ProductWriteError::ProductIdReservationExpired(
                reservation_id,
            ));
        }
        Ok(ProductIdReservationRow {
            id: row.get("id"),
            product_id: row.get("product_id"),
        })
    }
    pub(crate) async fn mark_reservation_consumed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        reservation_id: Uuid,
        product_record_id: Uuid,
    ) -> std::result::Result<(), ProductWriteError> {
        sqlx::query(
            r"
            UPDATE product_id_reservations
            SET status = 'consumed', consumed_at = NOW(), product_record_id = $2
            WHERE active_reservation_id = $1
            ",
        )
        .bind(reservation_id)
        .bind(product_record_id)
        .execute(tx.as_mut())
        .await
        .map_err(ProductWriteError::Database)?;
        Ok(())
    }
}
pub(crate) struct ProductIdReservationRow {
    pub(crate) id: Uuid,
    pub(crate) product_id: String,
}
