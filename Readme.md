# 草稿

日本語入力のための原稿用紙アプリです。

## 開発環境

推奨は Nix flakes を使う方法です。Rust、cargo-make、cargo-watch、rust-analyzer、Linux の実行に必要なライブラリなどは dev shell に含まれます。

```sh
nix develop
```

## 起動

```sh
cargo make dev
```

watch しながら起動する場合:

```sh
cargo make watch
```

直接 cargo で起動する場合:

```sh
SOUKOU_DEVELOPMENT_MODE=1 cargo run
```

## よく使うコマンド

```sh
cargo make check
cargo make test
cargo make fmt
```

個別に実行する場合:

```sh
cargo check --workspace
cargo test -p settings
cargo test -p keymap
cargo fmt
```

## 設定ファイル

設定ファイルは XDG ベースディレクトリ配下の `soukou` 設定ディレクトリに配置されます。

- `settings.json`: 表示や編集挙動の設定
- `keymap.json`: キーバインド上書き

設定ファイルを開く:

```sh
cargo make configure
```

## 新しい crate の作成

```sh
cargo make new <crate-name>
```

新しい crate を手で追加する場合は、`lib.rs` ではなく `Cargo.toml` の `[lib] path = "...rs"` でライブラリファイル名を明示してください。

