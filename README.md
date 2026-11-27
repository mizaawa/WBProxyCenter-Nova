# WBProxyCenter Nova

按冻结设计逐阶段重写的自托管测试服务。当前交付：M0 服务骨架和 M1 文件号池管理测试版。可设置管理员密码、登录、创建/导入/查询/编辑/导出/删除账号。账号仅在 access_account 文件库中。尚不转发对话，健康分组为未探测，不调用真实上游。完整产品范围见 REFACTOR-SPEC.md，实际交付以 CHANGELOG.md 为准。

## 运行 Actions 构建的测试包

默认分支推送触发 GitHub Actions「Build and test」。在成功的运行中下载 **wbpc-nova-linux-amd64**，解开外层 ZIP 后，在解压后的产物目录运行下方安装命令（Linux x86-64、Docker 和 Compose v2、当前用户可写 /www/wwwroot）。脚本创建私有子目录 wbproxycenter-nova-test，验证 SHA-256 后加载两个离线镜像，任何已有的同名目录都会被保留。该目录不应设成 Nginx/Apache 的静态根。账号和 Redis 单独存放在 Docker 私有卷中，不在可下载的网页目录里。

```sh
# 填入本机的 LAN IPv4 地址（默认回环）；端口默认 13040。
WBP_PUBLISH_ADDR=<测试服务器的内网IP> bash ./install-test.sh .
```

也可在自己的私有目录手动启动：

```sh
sha256sum -c wbpc-nova-linux-amd64.tar.gz.sha256
tar -xzf wbpc-nova-linux-amd64.tar.gz
sha256sum -c SHA256SUMS
docker load -i wbpc-nova-image.tar.gz
docker compose up -d --no-build --pull never
curl -fsS http://127.0.0.1:3040/health
```

默认只绑定回环；内网测试请用 WBP_PUBLISH_ADDR 设为服务器**实际的 LAN 地址**，WBP_PUBLISH_PORT 设为专用空闲端口（例如 13040）。不要把测试实例暴露到公网。Redis 不发布端口。停止用 `docker compose down`，**不要追加 -v**，那会删除账号和历史数据。

## 登录和测试

打开服务首页。在本机回环直连可直接设置管理员密码；从局域网或反向代理首次访问时，使用测试服务器上 `docker compose logs app` 输出的一次性设置码。不要将日志公开。至少 10 个字符，不要复用 SSH 密码。生产反代应使用 HTTPS，保留真实 Host 并设置 WBP_SECURE_COOKIE=1；测试 HTTP 只使用合成凭证。

号池页面支持 JSON 文件或粘贴、选区、分页筛选、编辑、启停、可确认的导出与删除。单次直接导入上限 8 MiB。超过 6 MiB 的多账号导出生成含 JSON 分片的 ZIP。补扫外部变更不超过 5 秒。批次上传、OAuth、上游探测、对话和 M2–M9 还未交付，不需要真实上游凭据即可验收本阶段。

## 源码与验证

MSRV 1.85，Rust 2021，Redis ≥7。九个库加一个可执行程序。在有 Redis 的机器上运行 `cargo run --locked`（仅 API），或先在 `web/` 运行 `npm ci && npm run build` 再执行 `cargo run --locked --features web-ui`（带面板）。默认 localhost:3040、凭证目录在可执行文件旁；可用 WBP_ADDR / WBP_ACCESS_DIR / WBP_DATA_DIR / WBP_REDIS_URL 修改。数据目录不要指向现有网站目录。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
```
