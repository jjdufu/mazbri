# 开发演示种子：默认不在 redep / 生产 init 执行。
# 用法（需本机 psql 与可达库）：
#   make -C docker/cwork/scripts seed-mock \
#     RATELIMIT_DB_URL=postgres://.../mazbri_ratelimit \
#     CACHE_DB_URL=postgres://.../mazbri_cache
#
# 若自行插入 mock 告警规则/事件，必须带 labels.seed=dev19-mock；
# 管理端 GET /alerts/events 默认 include_seed=false 会过滤它们。

.PHONY: seed-mock help

help:
	@echo "Targets: seed-mock (requires RATELIMIT_DB_URL and CACHE_DB_URL)"

seed-mock:
	@test -n "$(RATELIMIT_DB_URL)" || (echo "RATELIMIT_DB_URL required" >&2; exit 1)
	@test -n "$(CACHE_DB_URL)" || (echo "CACHE_DB_URL required" >&2; exit 1)
	psql "$(RATELIMIT_DB_URL)" -v ON_ERROR_STOP=1 -f dev19-mock-ratelimit-seed.sql
	psql "$(CACHE_DB_URL)" -v ON_ERROR_STOP=1 -f dev19-mock-cache-seed.sql
	@echo "seed-mock done (ratelimit+cache). Alert seed labels use seed=dev19-mock if you insert alerts manually."
