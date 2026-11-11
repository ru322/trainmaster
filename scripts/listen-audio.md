# 音を試聴して、音量を見直す(14 §6.7 の 6)

ゲームの音(BGM 4 曲、ジングル 3 つ、効果音 16)を人の耳で聞いて、「良い / 直す」と直す方向を記録する手順。あわせて、音量の暫定の値(14 §6.1 の `master` など)を見直すときに、どのファイルのどの値を変え、どう確かめるかを書く。

- 試聴は**開発機**で行う(この手順書)。**実機のスピーカーや HDMI での確認**は `scripts/av-poc-device.md` §6.1(4 番の `av_poc` で BGM と効果音)と、`scripts/verify-hardware.md` §9.2(会場の表示機器)で行う。そこで気づいたことも、この手順書の記録の表に書いてよい。
- 子どもに聞いてもらう分(うるさくないか、飽きないか)は `scripts/playtest.md` で行う。
- コマンドは、リポジトリのいちばん上のディレクトリ(`mise.toml` がある所)で打つ。

## 0. 用意するもの

| もの | 注意 |
|---|---|
| 開発機(Linux。`aplay` か `pw-play` で音が出るもの) | `aplay` は alsa-utils、`pw-play` は PipeWire に入っている |
| ヘッドホンと、小さなスピーカー | 両方で聞く。ヘッドホンは細かい雑音(「ボッ」「プツ」)、スピーカーは会場に近い聞こえ方 |
| 静かな部屋 | |
| 聞く人 2 人以上(できれば) | 1 人が「直す」と思っても、もう 1 人に聞いてもらう |

**耳を守る**: 鳴らす前に、開発機の音量を小さくしておき、少しずつ上げる。

## 1. 試聴用の WAV を作る

音は `assets/audio/<名前>.ron`(譜面)から `tm-audiogen` が作る。ゲームに入っている WAV は `crates/tm-assets/generated/audio/` にあり、同じ譜面からは毎回同じ WAV ができる。

**全部を作る**(リポジトリの外に出す。ゲームに入っている WAV は書きかえない):

```sh
mise exec -- cargo run --locked -q -p tm-audiogen -- build --out /tmp/tm-listen
ls /tmp/tm-listen
```

- 23 個の `.wav` と `audio_manifest.txt` ができる。最後に `23 個の音、WAV の合計 … バイト(予算の …%)` と出る。
- `--out` を付けないと、ゲームに入っている WAV(`crates/tm-assets/generated/audio`)を書きかえるので、**試聴では必ず付ける**。
- `audio_manifest.txt` の `ms` の列が音の長さ(ミリ秒)、`peak` の列が最大の振幅(32768 が上限)。

**1 つだけ作る**(譜面を変えて試すとき。出力先のディレクトリは先に作っておく):

```sh
mkdir -p /tmp/tm-listen
mise exec -- cargo run --locked -q -p tm-audiogen -- render assets/audio/se_coin.ron -o /tmp/tm-listen/se_coin.wav
mise exec -- cargo run --locked -q -p tm-audiogen -- render assets/audio/bgm_title.ron -o /tmp/tm-listen/bgm_title_x2.wav --loops 2
```

- `render` は `se_coin: 2646 サンプル(120 ms)、loop_start None、最大の振幅 11110` のような行を出す。
- BGM に `--loops 2` を付けると、ループして 2 周目に入るところまで続けて書く。**継ぎ目**(1 周目の終わり)は、`audio_manifest.txt` の `ms` の時刻(`bgm_title` なら約 30.1 秒)。
- 目安から外れると `warning[W3] se_coin.ron: 最大の振幅 14800 が目安 35% を超える` のような警告が出る(§8)。

**ゲームに入っている WAV と同じか**を確かめるとき:

```sh
cmp /tmp/tm-listen/se_buy.wav crates/tm-assets/generated/audio/se_buy.wav && echo 同じ
```

**注意**: WAV は、ゲームの中の音量(初期値で 音楽 6 = 0.36 倍、音 8 = 0.64 倍)を掛ける**前**の大きさ。WAV どうしを聞きくらべると、ゲームの中より BGM とジングルが大きく聞こえる。**音量のつり合いは §6 のとおり、ゲームか `av_poc` で聞く**。WAV では、音色・高さ・長さ・印象・雑音を聞く。

## 2. 鳴らし方

```sh
aplay -q /tmp/tm-listen/se_buy.wav               # alsa-utils
pw-play /tmp/tm-listen/se_buy.wav                # PipeWire(どちらか鳴る方)
```

効果音を 1 秒おきに順に鳴らす(名前が出てから鳴る):

```sh
for f in /tmp/tm-listen/se_*.wav; do echo "$(basename "$f")"; aplay -q "$f"; sleep 1; done
```

2 つの音を交互に 3 回鳴らす(聞き分け。§5.2):

```sh
ab() { for i in 1 2 3; do echo "$1"; aplay -q "/tmp/tm-listen/$1.wav"; sleep 0.7; echo "$2"; aplay -q "/tmp/tm-listen/$2.wav"; sleep 1.5; done; }
ab se_buy se_menu_open
```

波形を見たいとき(「ボッ」の位置、長さ)は、Audacity などで WAV を開く。

## 3. BGM(4 曲。01 §12.3、14 §6.3)

それぞれ `--loops 2` で作った WAV を最後まで聞く(§1)。

| 曲 | 使う場面 | 聞くこと |
|---|---|---|
| `bgm_title` | タイトル画面 | 明るく元気な行進曲。「これから始まるぞ」とワクワクするか |
| `bgm_game_1` | 会社 Lv0〜4 | のんびり明るく、説明を読むじゃまにならない静かさか。「ガタンゴトン」の線路の音に聞こえるか |
| `bgm_game_2` | 会社 Lv5〜11 | 1 と同じメロディで、にぎやかになったと分かるか |
| `bgm_game_3` | 会社 Lv12〜16 | 盛り上がって、フィナーレらしいか |

どの曲でも聞くこと:

- **継ぎ目**: ループの継ぎ目(`ms` の時刻)で、音が途切れたり「プツ」と鳴ったり、リズムがずれたりしないか。
- **口ずさめるか**: 1 回聞いた後、メロディの最初(汽笛のような 6 度の跳躍)を口ずさめるか。
- **3 曲のつながり**: `bgm_game_1` → `_2` → `_3` を続けて聞いて、「同じ曲が、会社が大きくなってにぎやかになった」と聞こえるか(14 §6.5)。
- **飽き**: `bgm_game_1` を 3 周(約 3.5 分)続けて聞いて、耳につく所(同じ音がしつこい、高い音が痛い)がないか(D-34。子どもの分は試遊で見る)。

## 4. ジングル(3 つ。01 §12.3、14 §6.4)

| ジングル | 鳴る場面 | 聞くこと |
|---|---|---|
| `jingle_company_up` | 会社レベルが上がった(約 2 秒) | 「やった!」と分かる短いファンファーレか |
| `jingle_all_unlocked` | 会社 Lv4(全部の機能が使える)(約 3 秒) | `jingle_company_up` より少し豪華に聞こえるか |
| `jingle_goal` | ゴール(1 無量大数円)(約 6 秒) | いちばん豪華で、明るく終わるか |

- 3 つを続けて鳴らし、豪華さの順(company_up < all_unlocked < goal)になっているか。
- 終わりの余韻が不自然に切れないか。

## 5. 効果音(16。01 §12.2、14 §6.2)

### 5.1 1 つずつ

§2 の `for` で順に鳴らし、記録の表 L3 の「音のイメージ」と合っているかを聞く。特に次を見る:

- `se_confirm`・`se_cancel`・`se_line_new`・`se_line_extend` は、波形の直流を取り除いたので、聞こえる大きさが約 4〜5dB 下がっている(14 §6.1)。**小さすぎないか**。
- 短い音(`se_cursor` 約 30ms、`se_confirm`・`se_cancel` 約 90ms)が「プチッ」という雑音に聞こえないか。

### 5.2 聞き分け

次の組を §2 の `ab` で鳴らし、目をつぶって聞いても、どちらか分かるかを試す(1 人が鳴らし、もう 1 人が当てる。順番を変えて 3 回)。

| 組 | 分かってほしい違い |
|---|---|
| `se_buy` と `se_menu_open` | 買えた / メニューを開いた(14 §6.7 が名指しした組) |
| `se_buy` と `se_buy_max` | 1 段 / まとめて |
| `se_confirm` と `se_cancel` | 決定 / キャンセル(上がる / 下がる) |
| `se_cursor` と `se_confirm` | 動いた / 決めた |
| `se_cannot_buy` と `se_error` | お金が足りない / できない操作 |
| `se_menu_open` と `se_menu_close` | 開く / 閉じる |
| `se_line_new` と `se_line_extend` | 路線ができた / のばした |
| `se_station_new` と `se_unit_up` | 駅ができた / 新しい数の単位 |
| `se_popup` と `se_confirm` | 説明が出た / 決定 |

## 6. 音量のつり合い(ゲームの中の大きさで聞く)

ゲームの中では、BGM とジングルは「音楽」の音量(初期値 6)、効果音は「音」の音量(初期値 8)を掛けて鳴る。14 §6.1 の目安(暫定決定 PL-D-23):

- ジングルは BGM より 4dB 以上大きい。
- ジングルは `se_buy` より 3dB 小さいところまでに収める(ほぼ同じ大きさ。**ご褒美の音が買った音に負けない**)。
- `se_coin` は `se_buy` より 2dB 以上小さい(何度も鳴るので控えめに)。

数字の上ではいまの値で満たしている(`tm-audiogen` のテストで確かめている)。ここでは**耳で**そう聞こえるかを確かめる。

### 6.1 `av_poc` で聞く(開発機)

```sh
mise exec -- cargo run --release -p tm-platform-sdl --example av_poc
```

ウィンドウが開き、ゲームと同じ初期の音量で、次が鳴り続ける(Esc で終わる):

- BGM: 最初は `bgm_title`。15 秒目に `bgm_game_1` へ、その後は 20 秒ごとに 2 曲が 1 秒のクロスフェードで入れかわる。
- 効果音: 100ms ごとに `se_cursor` → `se_confirm` → `se_coin` → `se_buy` → `se_popup` → `se_unit_up` → `se_cancel` → `se_error` の順。
- ジングル: 10 秒ごとに `jingle_company_up`。**鳴っている間は BGM が止まり、終わると止めた所から続く**。

聞くこと:

1. ジングルが、まわりで鳴る `se_buy` に負けずに「ご褒美」に聞こえるか。
2. ジングルが始まるとき(BGM を 5ms で止める)と、終わって BGM に戻るときに、「ボッ」「プツ」という音がしないか。ヘッドホンで、ジングルの始まりと終わりに耳をすます(効果音が鳴り続けているので、5 回以上聞く)。
3. BGM のクロスフェード(20 秒ごと)が自然か。

### 6.2 ゲームで聞く(開発機)

```sh
mise run run
```

ゲームのウィンドウが開く(開発用の機能つき)。キーは矢印・Z(決定)・X(もどる)・C(MAX)・B(ろせん)・Esc(せってい)。

| 場面 | 聞くこと |
|---|---|
| タイトル → ゲームを始める | `bgm_title` から `bgm_game_1` へ切りかわる |
| 強化を買う(Z)、MAX で買う(C) | `se_buy` / `se_buy_max` が気持ちよいか |
| 会社レベルが上がる | `jingle_company_up` → `se_station_new` → 説明の `se_popup`。ジングルで BGM が止まるとき・戻るときに「ボッ」がないか。ジングルが `se_buy` に負けないか |
| 路線が 3 本以上になって列車が多いとき | `se_coin` が続けて鳴っても、うるさくないか(同じ音は 150ms あけるので、多くても 1 秒に約 7 回) |
| せってい(Esc)を開く | BGM が半分の大きさになる(ダッキング)のが自然か |
| せってい の「音楽の 大きさ」「音の 大きさ」を 1 段ずつ変える | 初期値(6・8)のつり合いがよいか。もっと良い組があれば、その数を書く |
| せってい の「さいしょから やりなおす」 | `se_reset` の後に BGM が止まるとき「ボッ」がないか |

- 会社レベルが上がるまで待つのが長いときは、早送りで起動してよい: `TM_TIME_SCALE=4 mise run run`(ゲームの進みが 4 倍。列車が着く回数も 4 倍になるので、`se_coin` の数は実際より多い。`se_coin` のうるささは早送りなしでも聞く)。
- 終わるときは、ウィンドウを閉じる。

## 7. 記録の表

「良い / 直す」のどちらかに ○ を付け、直すときは**方向**を書く(例: 「もう少し小さく」「最後の音をもう少し長く」「`se_confirm` と似ているので高さを変える」)。数字が分かれば数字も(例: 「2dB くらい小さく」)。

### L0 共通

| 項目 | 結果 |
|---|---|
| 聞いた人(人数と、大人 / 子ども) | |
| 開発機の再生(`aplay` / `pw-play`)、ヘッドホン・スピーカーの種類 | |
| 実機で聞いたか(`av-poc-device.md` §6.1 / `verify-hardware.md` §9.2)、その機器 | |

### L1 BGM

| 曲 | 印象(良い / 直す) | 継ぎ目 | 口ずさめるか | 直す方向のメモ |
|---|---|---|---|---|
| `bgm_title` | | | | |
| `bgm_game_1` | | | | |
| `bgm_game_2` | | | | |
| `bgm_game_3` | | | | |
| 3 曲のつながり(だんだんにぎやか) | | — | — | |
| 3 周聞いて耳につく所 | | — | — | |

### L2 ジングル

| ジングル | 良い / 直す | 直す方向のメモ |
|---|---|---|
| `jingle_company_up` | | |
| `jingle_all_unlocked` | | |
| `jingle_goal` | | |
| 豪華さの順 | | |

### L3 効果音

| 音 | 意味・音のイメージ(01 §12.2) | 良い / 直す | 直す方向のメモ |
|---|---|---|---|
| `se_coin` | お金が入る。「チャリン」 | | |
| `se_buy` | 強化できた。上がる 3 音 | | |
| `se_buy_max` | まとめて強化。上がる 5 音 | | |
| `se_cannot_buy` | お金が足りない。「ブッ」 | | |
| `se_error` | できない操作。「ブブー」 | | |
| `se_station_new` | 駅ができた。「ポン」+ きらきら | | |
| `se_line_new` | 路線ができた。汽笛「ポッポー」 | | |
| `se_line_extend` | 路線をのばした。短い汽笛「ポッ」 | | |
| `se_confirm` | 決定。上がる 2 音 | | |
| `se_cancel` | キャンセル。下がる 2 音 | | |
| `se_cursor` | カーソル。「コッ」 | | |
| `se_menu_open` | メニューを開く。上がる 3 音 | | |
| `se_menu_close` | メニューを閉じる。下がる 3 音 | | |
| `se_unit_up` | 新しい数の単位。きらきら上がる | | |
| `se_popup` | 説明・お知らせ。「ポン」 | | |
| `se_reset` | リセット。下がっていく 4 音 | | |

| 聞き分け(§5.2) | 3 回のうち当たった数 | 直す方向のメモ |
|---|---|---|
| `se_buy` / `se_menu_open` | | |
| `se_buy` / `se_buy_max` | | |
| `se_confirm` / `se_cancel` | | |
| `se_cursor` / `se_confirm` | | |
| `se_cannot_buy` / `se_error` | | |
| `se_menu_open` / `se_menu_close` | | |
| `se_line_new` / `se_line_extend` | | |
| `se_station_new` / `se_unit_up` | | |
| `se_popup` / `se_confirm` | | |

### L4 音量のつり合いと雑音(§6)

| 項目 | 良い / 直す | 直す方向のメモ(どの音を、どれくらい) |
|---|---|---|
| ジングルが `se_buy` に負けていない(ご褒美に聞こえる) | | |
| ジングルが BGM よりはっきり大きい | | |
| `se_coin` が続けて鳴ってもうるさくない | | |
| 小さすぎる効果音がない(`se_confirm`・`se_cancel`・`se_line_new`・`se_line_extend`) | | |
| ジングルで BGM を止めるとき・戻るときの「ボッ」「プツ」 | | |
| リセットで BGM を止めるときの「ボッ」 | | |
| BGM のクロスフェード、せってい を開いたときのダッキング | | |
| 初期の音量(音楽 6・音 8)のつり合い。よりよい組があればその数 | | |

## 8. 音量の暫定の値を見直す

直すのは、音の担当のカード(領域 `assets/audio/**` と生成物)で行う。ここでは、記録の表の「直す」を、どの値を変えれば実現できるかを決め、試して確かめる流れを書く。**試すときは、元に戻せるように、変える前の値を記録の表に書いておく**。

### 8.1 どの値を変えるか

| 直したいこと | 変える値 | ファイル |
|---|---|---|
| 1 つの音を大きく / 小さく | その音の `master`(5 行目の `master: 140,`。百分率で、140 は 1.4 倍。0〜400) | `assets/audio/<名前>.ron` |
| BGM 全体・効果音全体の大きさ | 各ファイルの `master` をそろえて変える | 同上 |
| 初期の音量(音楽 6・音 8) | `INITIAL_BGM_LEVEL`・`INITIAL_SE_LEVEL` と、`Settings` の `music: 6`・`sound: 8` | `crates/tm-platform-sdl/src/audio/mixer.rs`、`crates/tm-ui/src/settings.rs`(01 §12.4 の決定なので、変えるならメインエージェントに出す) |
| 曲の中の 1 つの楽器(パート)の大きさ | そのチャンネルの `volume`、譜面の行の `v00`〜`v15` | `assets/audio/<名前>.ron` |
| 全部の曲で使う音色の大きさ | 音色の `volume` | `assets/audio/_lib/instruments.ron`(全曲が変わるので、なるべく使わない) |
| 音色・高さ・長さ・聞き分け | 譜面(`patterns`)と音色(`instruments`) | `assets/audio/<名前>.ron`(書き方は `assets/audio/README.md`) |

`master` と大きさの関係(`master` は掛け算):

| 大きさの変化 | `master` を何倍にするか | 例(140 のとき) |
|---|---|---|
| +1dB | 1.12 倍 | 157 |
| −1dB | 0.89 倍 | 125 |
| −2dB | 0.79 倍 | 111 |
| −3dB | 0.71 倍 | 99 |

### 8.2 上げられる上限を見る

最大の振幅には目安の上限がある(14 §6.1。`tm-audiogen` の警告 W3): BGM と効果音 50%、ジングル 90%、`se_coin` 35%。いまの値から、`master` をどこまで上げられるかを出す:

```sh
for f in assets/audio/[bjs]*.ron; do n=$(basename "$f" .ron); m=$(grep -o 'master: [0-9]*' "$f" | grep -o '[0-9]*$'); awk -v n="$n" -v m="$m" '$2 == n { lim = (n == "se_coin") ? 35 : ($3 == "Jingle") ? 90 : 50; p = $8; printf "%-20s master %3d  振幅 %4.1f%%(上限 %d%%) master の上限 %d\n", n, m, p * 100 / 32768, lim, int(m * lim * 32768 / 100 / p) }' crates/tm-assets/generated/audio/audio_manifest.txt; done
```

- ジングルはすでに上限(90%)の近くにある(`master` はあと数しか上げられない)。**「ジングルが `se_buy` に負ける」ときは、ジングルを上げるのではなく、`se_buy`・`se_buy_max`(とほかの効果音)の `master` を下げる**。
- 上限を超えると W3 の警告が出て、§8.3 のテストが失敗する。

### 8.3 試して確かめる流れ

1. `assets/audio/<名前>.ron` の値を変える。
2. その音だけ作り直して聞く(§1 の `render`)。警告が出ないかを見る。
3. 音のテストを回す(切り詰め、振幅の範囲、大きさのつり合い、長さ、ループの継ぎ目):
   ```sh
   mise exec -- cargo test --locked -p tm-audiogen --test assets_audio
   ```
   - `test result: ok. 10 passed` なら目安の中。
   - 失敗の例: `ジングル ≥ se_buy − 3dB: …`、`se_coin ≤ se_buy − 2dB: …`、`se_buy: -9.5 dBFS`(効果音の大きさの範囲 −19〜−10dBFS を外れた)、`最大の振幅 …%`、`[Diagnostic { code: W3, … }]`。
   - 耳で決めた値が目安(14 §6.1、PL-D-23)を外れるときは、目安そのものを変える提案になる。値と理由(記録の表)を添えてメインエージェント(14 の担当)に出す。テストの目安は `tools/tm-audiogen/tests/assets_audio.rs` の `loudness_is_balanced` と `no_clipping_and_peaks_in_range`。
4. ゲームに入れる WAV を作り直す:
   ```sh
   mise run assets:gen
   ```
5. 作り直しが済んでいるかを確かめる:
   ```sh
   mise run assets:check      # assets:check: up to date と出れば済んでいる
   ```
   作り直しを忘れていると `assets:check: generated assets are out of date; run 'mise run assets:gen' and commit` と出て失敗する(CI でも同じ検査をする)。
6. §6 の `av_poc` とゲームで、つり合いを聞き直す。実機でも聞く(`scripts/av-poc-device.md` §6.1 の 4)。
7. 14 §6.1 の「いまの `master`」と「いまの値での実測」の行を、新しい値に合わせて直してもらう(設計書は 14 の担当が直す)。`.ron` と、生成した `crates/tm-assets/generated/audio/` の WAV と `audio_manifest.txt` を一緒にコミットする。

### 8.4 直す方向の決め方(よくある例)

| 記録の表のメモ | まず試すこと |
|---|---|
| ジングルが `se_buy` に負ける | `se_buy`・`se_buy_max` の `master` を 1〜2dB 下げる(§8.2) |
| `se_coin` がうるさい | `se_coin` の `master` を下げる(振幅の下限 20% まで)。それでもうるさければ、間引き(150ms)を長くする案をメインエージェントに出す(01 §12.2 の決定) |
| BGM が大きくて効果音が聞こえない | 4 曲の `master` をそろえて下げる。4 曲の大きさの差は 2.5dB 以内(テスト) |
| ある効果音が小さすぎる | その音の `master` を上げる(§8.2 の上限まで) |
| 「ボッ」「プツ」が聞こえる | 値ではなく作り方の問題。鳴った場面(ジングルの始まり / 終わり、リセット)と、どの曲のときかを書いて、14 の担当に出す(14 PL-D-30 の直流の取り除き、§4.3 の 5ms のフェード) |
| 2 つの効果音が聞き分けられない | どちらか一方の高さか音色を変える。どちらを変えるかの案を書いて音の担当に出す |
