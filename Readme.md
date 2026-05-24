# 草稿

思考の速度で編集する。日本語入力のための軽量で高速な原稿用紙アプリ。

## ローカル開発でログインする

`genko` のログインは `soukou.dev` と Supabase Auth を経由します。ローカルで試すときは `soukou.dev` と `soukou-supabase` も一緒に立ち上げてください。

1. `soukou-supabase` で `supabase start` を実行する。
2. `supabase status` の `API URL` と `anon key` を確認する。
3. `genko/.env` に少なくとも次を設定する。

```dotenv
SOUKOU_SITE_URL=http://localhost:3000
SOUKOU_SUPABASE_URL=http://127.0.0.1:54321
SOUKOU_SUPABASE_PUBLISHABLE_KEY=<supabase status の anon key>
SOUKOU_AUTH_CALLBACK_SCHEME=soukou
```

4. `soukou.dev/.env` にも同じ Supabase 情報を設定する。

```dotenv
VITE_SITE_URL=http://localhost:3000
VITE_SUPABASE_URL=http://127.0.0.1:54321
VITE_SUPABASE_PUBLISHABLE_KEY=<supabase status の anon key>
SITE_URL=http://localhost:3000
SUPABASE_URL=http://127.0.0.1:54321
SUPABASE_SERVICE_ROLE_KEY=<supabase status の service_role key>
```

5. `soukou.dev` を `npm run dev` で起動し、`genko` を起動する。

補足:
- local Supabase で Google ログインまで使う場合は、`soukou-supabase/supabase/config.toml` に加えて Google OAuth の `client_id` / `secret` 設定が必要です。
- 既存の hosted Supabase を使う場合も、Auth の redirect allowlist に `http://localhost:3000/signin` と `http://localhost:3000/auth/native/callback` を追加してください。

## roadmap

### Version 0.2

- [ ] 日本語IME入力
- [ ] 変換中表示
- [ ] 変換確定
- [ ] 連続入力
- [ ] 長文入力で破綻しない
- [ ] かな/英数切り替え
- [ ] 絵文字でクラッシュしない
- [ ] サロゲートペア対応
- [ ] 合成文字対応
- [ ] キャレット位置が正しい
- [ ] IME候補位置が正しい
- [ ] スクロール時に追従
- [ ] ページ跨ぎで壊れない
- [ ] 縦書きで違和感ない
- [ ] Backspace
- [ ] Delete
- [ ] Enter
- [ ] 改行
- [ ] 範囲選択
- [ ] コピー
- [ ] カット
- [ ] ペースト
- [ ] Undo
- [ ] Redo
- [ ] マス目がズレない
- [ ] リサイズで壊れない
- [ ] スクロールでちらつかない
- [ ] 高DPIで綺麗
- [ ] 長文でFPS低下しすぎない
- [ ] マス目がズレない
- [ ] リサイズで壊れない
- [ ] スクロールでちらつかない
- [ ] 高DPIで綺麗
- [ ] 長文でFPS低下しすぎない
- [ ] 句読点位置
- [ ] 括弧回転
- [ ] 英数字の扱い
- [ ] 中黒
- [ ] 三点リーダ
- [ ] ダッシュ
- [ ] 禁則処理（最低限）
- [ ] ページ送り
- [ ] ページ境界表示
- [ ] スクロール位置保持
- [ ] ページ増減で壊れない
- [ ] 新規作成
- [ ] 保存
- [ ] 名前を付けて保存
- [ ] 再読み込み
- [ ] 自動保存
- [ ] クラッシュ後復帰（できれば）
- [ ] UTF-8
- [ ] 改行コード混在で壊れな
- [ ] 数十万文字で固まらない
- [ ] リサイズ
- [ ] 最小化
- [ ] フルスクリーン
- [ ] 複数ウィンドウ（できれば）
- [ ] Cmd+S
- [ ] Cmd+Z
- [ ] Cmd+Shift+Z
- [ ] Cmd+C
- [ ] Cmd+V
- [ ] Cmd+X
- [ ] Cmd+A
- [ ] About
- [ ] Quit
- [ ] Save
- [ ] Edit
- [ ] 空文字
- [ ] 超長文
- [ ] IME変換中削除
- [ ] ページ境界
- [ ] ウィンドウ縮小
- [ ] 高速スクロール
- [ ] 高速Undo
- [ ] unwrap() 減らす
- [ ] panicで落ちない
- [ ] ログ出力ある
- [ ] release build確認
- [ ] アプリ化
- [ ] .app 化
- [ ] アイコン
- [ ] アプリ名
- [ ] バージョン
- [ ] 文字数表示
- [ ] 枚数表示
- [ ] 行数設定
- [ ] 1ページ字数設定
