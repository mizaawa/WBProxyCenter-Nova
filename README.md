# WBProxyCenter Nova

按冻结设计逐阶段重写的自托管测试服务。当前里程碑：M0。健康和版本接口可用；尚不能转发对话。完整产品范围见 REFACTOR-SPEC.md，实际交付以 CHANGELOG.md 为准。

## 运行 Actions 构建的测试包

默认分支推送触发 GitHub Actions「Build and test」。在成功的运行中下载 **wbpc-nova-linux-amd64**，解开外层 ZIP 后，把两个文件放到测试目录（Linux x86-64、Docker 和 Compose v2）。该目录不应设成 Nginx/Apache 的静态根。账号和 Redis 单独存放在 Docker 私有卷中，不在可下载的网页目录里。

```sh
sha256sum -c wbpc-nova-linux-amd64.tar.gz.sha256
tar -xzf wbpc-nova-linux-amd64.tar.gz
sha256sum -c SHA256SUMS
docker load -i wbpc-nova-image.tar.gz
docker compose up -d
curl -fsS http://127.0.0.1:3040/health
```

默认只绑定回环；内网测试请用 WBP_PUBLISH_ADDR 设为服务器**实际的 LAN 地址**，WBP_PUBLISH_PORT 设为专用空闲端口（例如 13040）。不要把测试实例暴露到公网。Redis 不发布端口。停止用 `docker compose down`，**不要追加 -v**，那会删除账号和历史数据。

## 源码与验证

MSRV 1.85，Rust 2021，Redis ≥7。九个库加一个可执行程序。在有 Redis 的机器上运行 `cargo run --locked`。默认 localhost:3040、凭证目录在可执行文件旁；可用 WBP_ADDR / WBP_ACCESS_DIR / WBP_DATA_DIR / WBP_REDIS_URL 修改。数据目录不要指向现有网站目录。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
```
