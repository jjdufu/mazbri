#!/usr/bin/env bash
# 门禁 2：本机全量编译 + Rust 后端契约检验 + 容器 /tmp 原子热替换 + 科学轻量磁盘清理
# 严禁触发 GitHub Actions CI；严禁 docker compose up -d；严禁使用 MAZBRI_SKIP_FRONTEND_EMBED。
#
# 用法：
#   bash .migration/gates/gate-2-local-hotswap.sh
#   MGMT_CONTAINER=mazbri-max bash .migration/gates/gate-2-local-hotswap.sh
#   bash .migration/gates/gate-2-local-hotswap.sh --skip-docker

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
ACTIVE_FILE="$REPO_ROOT/.migration/ACTIVE"
if [[ ! -f "$ACTIVE_FILE" ]]; then
  echo "❌ [EXPIRED] .migration/ACTIVE 不存在：迁移已结束，门禁 2 已失效。" >&2
  exit 2
fi

SKIP_DOCKER=0
for arg in "$@"; do
  if [[ "$arg" == "--skip-docker" ]]; then
    SKIP_DOCKER=1
  fi
done

# 加载本地 .migration/.env.local（若存在）
if [[ -f "$REPO_ROOT/.migration/.env.local" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$REPO_ROOT/.migration/.env.local"
  set +a
fi

TARGET_URL="${TARGET_URL:-http://192.168.50.50:8085}"
unset MAZBRI_SKIP_FRONTEND_EMBED || true

echo "=== [GATE-2 Step 1/6] 运行门禁 0（环境与密钥卫生） & 门禁 1（静态铁律） ==="
node "$REPO_ROOT/.migration/gates/gate-0-env-hygiene.mjs"
node "$REPO_ROOT/.migration/gates/gate-1-static-laws.mjs"

echo "=== [GATE-2 Step 2/6] 前端类型检查、构建与全仓单元测试 ==="
cd "$REPO_ROOT/mazbri-web"
pnpm exec tsc --noEmit
pnpm build
pnpm test
# 额外扫描全仓 *.test.ts 确保无漏网单测
while IFS= read -r test_file; do
  [[ -z "$test_file" ]] && continue
  node --import tsx --test "$test_file"
done < <(find src -name '*.test.ts' | sort)

echo "=== [GATE-2 Step 3/6] 后端 Release 编译（嵌入新版 dist）与 Rust 契约测试 ==="
cd "$REPO_ROOT/mazbri-bin"
cargo build --release --locked -p mazbri-mgmt
cargo test --release --locked -p mazbri-mgmt --no-fail-fast

echo "=== [GATE-2 Step 4/6] 校验 ts-rs 自动生成类型零漂移 ==="
cd "$REPO_ROOT"
git diff --exit-code -- mazbri-web/src/types/api/generated || {
  echo "❌ mazbri-web/src/types/api/generated 存在未提交漂移，请先核对前后端类型！" >&2
  exit 1
}

if [[ "$SKIP_DOCKER" -eq 0 ]]; then
  echo "=== [GATE-2 Step 5/6] 原子热替换运行中 Docker 容器二进制（严禁 docker compose up -d） ==="
  CONTAINER="${MGMT_CONTAINER:-}"
  if [[ -z "$CONTAINER" ]]; then
    CONTAINER="$(docker ps --format '{{.Names}}' | grep -E '(^|[-_])(mazbri-)?(mgmt|max)($|[-_])' | head -n 1 || true)"
  fi
  if [[ -z "$CONTAINER" ]]; then
    echo "❌ 未自动探测到运行中的 mazbri-mgmt / mazbri-max 容器，请通过 MGMT_CONTAINER=<容器名> 指定。" >&2
    exit 1
  fi
  echo "→ 目标容器：$CONTAINER"
  BIN_PATH="$REPO_ROOT/mazbri-bin/target/release/mazbri-mgmt"
  docker cp "$BIN_PATH" "${CONTAINER}:/tmp/mazbri-mgmt"
  docker exec -u 0 "$CONTAINER" sh -c 'chmod 755 /tmp/mazbri-mgmt && mv -f /tmp/mazbri-mgmt /usr/local/bin/mazbri-mgmt'
  docker restart "$CONTAINER"

  echo "→ 等待 ${TARGET_URL}/health 返回 200..."
  HEALTH_OK=0
  for i in $(seq 1 30); do
    if curl -fsS --max-time 3 "${TARGET_URL}/health" >/dev/null 2>&1; then
      HEALTH_OK=1
      break
    fi
    sleep 1
  done
  if [[ "$HEALTH_OK" -ne 1 ]]; then
    echo "⚠️  ${TARGET_URL}/health 30 秒内未返回 200，请检查容器日志：docker logs --tail 50 $CONTAINER" >&2
    exit 1
  fi
  echo "✅ 容器热替换完成且健康检查通过。"
else
  echo "=== [GATE-2 Step 5/6] 已按 --skip-docker 跳过容器热替换 ==="
fi

echo "=== [GATE-2 Step 6/6] 轻量磁盘清理（保留 target/release/deps 维持秒级增量编译） ==="
rm -rf "$REPO_ROOT/mazbri-bin/target/debug" "$REPO_ROOT/mazbri-web/node_modules/.vite" "$REPO_ROOT/.migration/artifacts"/* 2>/dev/null || true
if [[ "$SKIP_DOCKER" -eq 0 ]] && command -v docker >/dev/null 2>&1; then
  # 仅清理悬空镜像与无用构建缓存；严禁使用 -a 误删运行中或基线镜像
  docker image prune -f >/dev/null 2>&1 || true
  docker builder prune -f >/dev/null 2>&1 || true
fi

echo "→ 磁盘快照："
df -h "$REPO_ROOT" | tail -n 1
du -sh "$REPO_ROOT/mazbri-bin/target" "$REPO_ROOT/mazbri-web/node_modules" 2>/dev/null || true
echo "✅ [GATE-2 PASSED] 编译、契约检验、热替换与轻量磁盘清理全部完成。"
