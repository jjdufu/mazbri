# 管理端静态资源缓存（index.html）

发版后旧 hash chunk 会 404。前端已在 `installChunkHardRefresh` 对动态 import / `vite:preloadError` 给出「硬刷新」提示，但网关仍应避免把入口 HTML 长期缓存，否则用户会长时间拿着指向旧 chunk 的壳。

建议（nginx 示例，按实际路径调整）：

```nginx
# 入口 HTML：协商或短缓存，禁止长期 immutable
location = /index.html {
  add_header Cache-Control "no-cache, no-store, must-revalidate";
}

# 带 content-hash 的构建产物可长期缓存
location /assets/ {
  add_header Cache-Control "public, max-age=31536000, immutable";
}
```

部署仓若另有网关配置，请与上述语义对齐；本文件仅作契约说明，不替代部署仓改动。
