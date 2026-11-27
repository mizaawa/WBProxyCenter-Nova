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

未设置管理员时，直接打开首页，填写「管理员密钥」和「确认管理员密钥」，点击「设置并登录」即可进入控制台。无需用户名、验证码或查看日志。两次输入必须一致，至少 10 个字符、最多 72 个 UTF-8 字节；不要复用 SSH 密码。此后登录只输入一次管理员密钥。升级不会重置已设置的密钥。

**首次设置前请限制访问。** 第一个能访问未初始化实例的人就能设置管理员。先在回环或仅管理员可访问的网络内完成设置，再向其他人开放；也可在首次启动时通过 `WBP_ADMIN_PASSWORD` 预设密钥，直接进入普通登录。生产反代应使用 HTTPS，保留真实 Host 并设置 `WBP_SECURE_COOKIE=1`；测试 HTTP 只使用合成凭证。

本项目不加密本地数据。`access_account/` 与 Redis 数据等同于凭证本身，请自行限制目录权限、做好备份、不要把数据目录暴露给他人。

号池页面支持 JSON 文件或粘贴、选区、分页筛选、编辑、启停、可确认的导出与删除。单次直接粘贴 JSON 上限 8 MiB；文件选择支持多文件 multipart，单批上限 256 MiB。服务收到文件后在私有 inbox 中处理，刷新或关闭页面不取消批次；回执保留 24 小时。超过 6 MiB 的多账号导出生成含 JSON 分片的 ZIP。补扫外部变更不超过 5 秒。OAuth、上游探测、对话和 M2–M9 还未交付，不需要真实上游凭据即可验收本阶段。

## 源码与验证

MSRV 1.85，Rust 2021，Redis ≥7。九个库加一个可执行程序。在有 Redis 的机器上运行 `cargo run --locked`（仅 API），或先在 `web/` 运行 `npm ci && npm run build` 再执行 `cargo run --locked --features web-ui`（带面板）。默认 localhost:3040、凭证目录在可执行文件旁；可用 WBP_ADDR / WBP_ACCESS_DIR / WBP_DATA_DIR / WBP_REDIS_URL 修改。数据目录不要指向现有网站目录。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
```
