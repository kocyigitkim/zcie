# ZCIEngine (ZCIE) — Altyapı ve Kural Seti Özeti

Bu belge, projeyi devralacak başka bir yapay zekâ için hazırlandı. Kodun nasıl çalıştığını, hangi kurallara uyulması gerektiğini ve bugünkü durumu tek yerde anlatır. Ayrıntıların asıl kaynağı `docs/` klasörü ve `docs/adr/` kararlarıdır; bu belge onların özetidir. Durum tarihi: 2026-10-17.

---

## 1. Ürün nedir?

ZCIE (ZeroContext Intelligence Engine), Claude Code ve OpenCode için **yerel çalışan bir proje hafızası ve kod indeksidir**.

- Her yeni oturum, önceki oturumların olgularını, kararlarını, kurallarını ve kullanıcının fikirlerini **model çağrısı yapmadan ve ağa çıkmadan** alır.
- Kod indeksi sayesinde model, dosya taramadan doğru dosyaya, sembole ve satır aralığına gider.
- Ana hedef: **daha az token ile daha doğru iş**. Arama, sıralama, grafik gezinme ve toplama işleri modelde değil, yerel daemon'da yapılır; modele ham veri değil sonuç verilir.

---

## 2. Mimari

### 2.1 Süreçler ve programlar

| Program | Görev |
|---|---|
| `zcied` | Tek yazıcı daemon. Veritabanını yalnızca o açar (`run/daemon.lock` ile özel kilit). HTTP API'yi `127.0.0.1:47831` adresinde sunar. |
| `zcie` | CLI (kurulum, sorgu, proje yönetimi, içe aktarma, doğrulama). |
| `zcie-mcp` | MCP sunucusu (stdio). İki araç sunar: `zcie_query`, `zcie_expand`. HTTP API'yi çağırır. |
| Claude Code hook'ları | `~/.zcie/hooks/*.sh` (sh + curl + jq). Olay yakalar, her isteme bağlam kapsülü ekler. |
| OpenCode eklentisi | `adapters/opencode/zcie.js`; hook'larla aynı kurallar. |
| SDK'lar | Rust (`sdk/rust`), TypeScript (`sdk/typescript`, bağımlılıksız), Python (`sdk/python`, yalnız standart kütüphane). |

Tüm istemciler yalnızca HTTP API (veya HTTP'yi çağıran MCP) üzerinden konuşur (ADR-003).

### 2.2 Rust crate'leri (`crates/`)

`zcie-types` (ortak tipler), `zcie-store` (RocksDB + blob deposu), `zcie-ingest` (iş kuyruğu, işleyiciler), `zcie-parse` (tree-sitter), `zcie-symbols`, `zcie-git` (git CLI), `zcie-fs` (tarama, izleyici), `zcie-search` (Tantivy), `zcie-vector` (yerel gömme), `zcie-memory` (hafıza kayıtları, görevler), `zcie-graph` (kenarlar), `zcie-query` (sorgu planlayıcı), `zcie-context` (kapsül/bütçe), `zcie-cache` (L0 LRU), `zcie-security` (anahtarlar, gizli bilgi temizleme, hariç tutma), `zcie-api` (Engine), `zcie-core` (yapılandırma, başlatma, izleyiciler, bakım), `zcie-runtime`, `zcie-telemetry`, `zcie-lsp`.
Uygulamalar: `apps/zcied`, `apps/zcie`, `apps/zcie-inspect`. Ağ geçitleri: `gateways/http`, `gateways/mcp` (+ `grpc`, `ipc` yer tutucu).

### 2.3 Depolama

- **RocksDB = tek gerçek kaynak** (canonical). Column family'ler: `registry`, `events`, `entities`, `edges_out`, `edges_in`, `revisions`, `lookup`, `jobs`, `policies`.
- Tüm değerler JSON'dur ve `"v"` (şema sürümü) alanı taşır; kodlama yalnızca `zcie-store/src/codec.rs` içinde (ADR-002).
- **Blob deposu**: içerik adresli, `objects/<hex[0..2]>/<hex[2..4]>/<hex>.zst`, BLAKE3 hash (`b3:<hex>`), zstd seviye 3. Olay yükleri ve indekslenen dosya içerikleri burada. Yetim blob temizliği dosya içeriklerini korur (ADR-016).
- **Türetilmiş indeksler** (ADR-005): Tantivy (sözcük araması) ve bellek içi vektör indeksi. Her birinin kendi su seviyesi (watermark) vardır: `canonical_seq`, `graph_seq`, `search_seq`. Başlangıçta eksik kayıtlar yeniden oynatılır veya indeks yeniden kurulur.
- Veri kökü: `~/.zcie` (`ZCIE_HOME` ile değişir). Örnek yollar: `~/.zcie/instances/local` (veri), `~/.zcie/config/` (`engine.toml`, `setup.toml`, `client.json`, `admin.key`, `claude-code.key`), `~/.zcie/hooks/`, `~/.zcie/models/`, `~/.zcie/bin/zcied` (launchd'nin çalıştırdığı kopya).

### 2.4 İş kuyruğu

- Tek `Mutex` + iki `VecDeque` (etkileşimli / arka plan) + `Condvar`; işçiler OS thread'leri (varsayılan 4, kurulumda seçilir).
- Bakım thread'i 2 saniyede bir bekleyen işleri kuyruğa alır, saatte bir yetim blob temizliği yapar.
- İşleyiciler batch'lerini tek yazma kilidi altında yazar; dosya okuma/hash/parse kilitten önce yapılır.

---

## 3. Proje kimliği (ADR-009, ADR-015)

- Proje = registry'deki workspace kaydı. ID: `proj_<ULID>` (eski `ws_` ID'ler geçerli).
- ID dosyası: `<proje kökü>/.claude/zcie_id` (tek satır). Proje kökü = Git kökü, yoksa klasörün kendisi.
- Arama: istemci bulunduğu klasörden yukarı çıkar; ilk geçerli `zcie_id` kazanır. **Arama bir Git kökünde durur** (iç içe repo kendi projesidir).
- `ZCIE_WORKSPACE` açık geçersiz kılmadır (testler ve benchmark'lar kullanır).
- Proje **asla** oluşturulmaz: ev klasörü (`$HOME`), `/`, ve geçici klasör kökleri (`$TMPDIR`, `/tmp`, `/var/tmp` ve `/private` biçimleri).
- SessionStart hook'u, ID'si olmayan klasör için projeyi otomatik açar ve ID dosyasını yazar.

---

## 4. İndeksleme

### 4.1 Hangi klasörler indekslenir (ADR-015)

- **Yalnızca `zcie project add <klasör>` ile eklenen klasörler** taranır, indekslenir ve izlenir (`WorktreeRecord.indexed = true`).
- İçe aktarma, hook'lar, `zcie claude setup`, `zcie project init` klasörü **yalnızca hafıza** için kaydeder.
- Bir kayıt `indexed` değerini asla geri `false` yapmaz.
- Başlangıçta indekslenmeyen klasörlerin bekleyen dosya/tarama işleri silinir.
- İzleyici `.gitignore` ve `.git/info/exclude` kurallarını uygular. `project add`, içteki Git depolarını ve `.gitignore` ile dışlanan üst klasörleri raporlar; `--nested` iç depoları ayrı proje olarak ekler.

### 4.2 Her zaman hariç tutulanlar (`zcie-security/src/exclude.rs`)

`**/.env`, `**/.env.*`, `**/*.pem`, `**/*.key`, `**/*.p12`, `**/id_rsa*`, `**/id_ed25519*`, `**/.aws/**`, `**/.ssh/**`, `**/secrets/**`, `**/node_modules/**`, `**/target/**`, `**/.git/**`, `**/.claude/settings*.json*`, `**/*.zcie-backup`. Ayrıca indekslenen metinlerde gizli bilgi temizleme (`redact_secrets = true`).

### 4.3 Dosya işleme

- Değişmeyen dosya (aynı hash, aynı ayrıştırıcı sürümü, blob mevcut) parse ve blob yazımı yapılmadan atlanır. Blob'u eksik dosya bir sonraki taramada yeniden indekslenir.
- Ayrıştırıcı: tree-sitter (Rust, TypeScript, Python). `PARSER_VERSION = 3`. Semboller: fonksiyon, tip, metot, sabit (sabitler değeriyle) vb.; modül bildirimleri haritada yok sayılır.
- **Dosya referansları** (`lookup: t/file_refs/<file id>`): dosyanın kullandığı isimler ve sayıları (en çok 500). Depo haritasının kenarlarını üretir.
- **Bağlam metni** (D-059, modelsiz "contextual retrieval"): her sembol dokümanına yol kelimeleri, kapsayıcılar (modül/tip) ve dosyanın ilk yorum satırı eklenir; uzun metin pasajları kendi Markdown başlığını taşır. Gösterilen başlık ve özet değişmez.
- Uzun metin: 1.500 karakterlik pasajlar, 200 karakter örtüşme (ADR-013).
- **Kod sırası** (`lookup: t/code_seq/<workspace>`): bir dosya değiştiğinde güncellenir; depo haritası önbelleği yalnızca buna göre yenilenir.

### 4.4 Vektör araması (ADR-011)

- Model2vec statik gömme, varsayılan çok dilli `potion-multilingual-128M` (alternatif İngilizce `potion-base-8M`). Ağ çağrısı yok; model `~/.zcie/models/` içinde.
- Gömülenler: semboller (isim + imza + yorumlar + yol + bağlam satırı), hafıza kayıtları (soru/gözlem olanlar hariç), görevler, commit'ler. Dosyalar ve ham olaylar gömülmez.
- İndeks bellekte tutulur; daemon açılışında depodan doldurulur (canlı depoda ~302.000 vektör, ~68 sn).

### 4.5 Türkçe desteği (ADR-013)

Harf katlama (ç→c, ğ→g, ı/İ→i, ö→o, ş→s, ü→u), kesme işareti eklerini atma ("API'sine"→"API"), 5 karakterlik kök (F5). Türkçe/İngilizce eşdeğer sözlüğü (`synonyms.rs`). Meta kelimeler ("kullanıcının", "istedi", "cevap ver" vb.) terim sayılmaz.

---

## 5. Hafıza ve grafik

### 5.1 Olaylar

Tipler: `user.message`, `assistant.message`, `tool.call`, `tool.result`, `command.run`, `test.result`, `file.changed`, `file.deleted`, `git.commit`, `session.start`, `session.end`, `user.confirmation`. Her olayın `idempotency_key`'i vardır (proje bazlı). Yük 1 MB'a kadar.

### 5.2 Hafıza kayıtları

- Türler: `fact`, `user_requirement`, `constraint`, `design_decision`, `task`, `observation`, `hypothesis`, `correction`, `preference`.
- Durumlar: `active`, `verified`, `disputed`, `superseded`, `retracted`.
- Tier A çıkarımı (regex, model yok): görev referansları (`#89`, `JIRA-12`), yollar, sembol isimleri, SHA'lar, hata satırları, kararlar ("We decided", "Karar:"), tercihler ("Always…").
- Düzeltmeler: yeni ifade eskisini `superseded` yapar; eski kayıt kapsülde "düzeltme kazanır" notuyla son sırada gösterilir.
- Takip mesajları (ADR-013): en iyi kullanıcı isteklerine oturumdaki sonraki kısa mesajlar (`follow_up`) ve önceki mesaj (`context`) eklenir. Zaten sonuç olarak gösterilen bir metin tekrar edilmez.
- Uzun/yapıştırılmış belge (2.000+ karakter veya `<pasted_content>`) görev üretmez.
- Asistan cevapları (ADR-016): Stop hook'u `last_assistant_message`'ı (en çok 30.000 karakter) kaydeder; ayar `answers` (varsayılan açık).

### 5.3 Grafik kenarları

İlişkiler: `REQUESTED_BY`, `IMPLEMENTS`, `AFFECTS_FILE`, `DEFINES_SYMBOL`, `CALLS` (şu an üretilmiyor), `IMPORTS`, `REFERENCES`, `TESTED_BY`, `FAILED_IN`, `FIXED_BY`, `CHANGED_IN_COMMIT`, `DERIVED_FROM`, `DECIDED_IN`, `CONTRADICTS`, `SUPERSEDES`, `BELONGS_TO`, `RELATED_TO`.
Durumlar ve ağırlıklar: `verified` 1.0, `inferred` 0.5, `candidate` 0.2, `rejected` 0.
Kanıt türleri: client_trace, user_confirmation, commit_trailer (`ZCIE-Task: #89`), test_outcome, git_history, derivation, ast_resolved, ast_approximate, text_mention, temporal.

---

## 6. Sorgu motoru (`crates/zcie-query`)

### 6.1 Sınıflar ve modlar

- Sınıflar: `exact_ref`, `symbol_definition`, `callers`, `timeline`, `implementation`, `task`, `search`.
- Modlar: `auto`, `exact`, `search`, `timeline`, `memory`, **`map`** (depo haritası), **`locate`** (değişecek dosyalar ve satır aralıkları).

### 6.2 Akış (`planner.rs`)

1. Soru 1–2.000 karakter; bütçe sınırlanır (varsayılan 250, en çok 4.000 token; hook 1.200 kullanır); en çok 50 sonuç.
2. L0 önbellek (anahtar: kiracı, kişi, proje, politika, soru, mod, bütçe, su seviyeleri).
3. `map` modu → depo haritası. `locate` modu **veya** "which file must I change / hangi dosyayı değiştirmeli" gibi ifadeler → konum cevabı. (Geçmiş zaman "değiştirdi" konum değildir.)
4. Sınıflandırma; "tüm kararları listele" → listeleme cevabı; "bu proje hakkında / project overview" → proje kartı.
5. Sınıf yolu (route) adayları + **grafik sıralaması** (yalnızca `search`, `timeline`, `task` sınıflarında; kesin sınıflarda değil).
6. Overlay kontrolü (henüz indekslenmemiş değişiklikler), yankı (sorunun kendisi) atma, sıralama, metin tekrarı atma.
7. Kompozisyon: yerel kısa cevap (yalnızca kesin veriden: tek sembol tanımı, tek görev+commit vb.), sıkıştırma, takip mesajları, kapsül kuralları, sayfalama.

### 6.3 Arama ve sıralama

- Hibrit: Tantivy (tam + kök sorgusu) + vektör, RRF ile birleşir; skor = sözcük payı + terim kapsama + sınıf bonusu, durum, yenilik ve tür ağırlıklarıyla çarpılır. Araç günlükleri cezalandırılır.
- `match_coverage`: tek bir sonucun içerdiği soru terimlerinin en iyi oranı (0–1); en az 2 terim eşleşmeli.
- **Grafik sıralaması** (D-060, HippoRAG): sorudaki görev referansları, belirgin sembol isimleri, yollar ve SHA'lar tohum olur; derinlik 3, en çok 2.000 düğüm, düğüm başına 50 kenar; Kişiselleştirilmiş PageRank (sönüm 0.5, 20 tur); ilk 10 düğüm aday olur; skor = 0.6 × √(rank / tohum dışı en iyi rank) (+0.2 köprü bonusu).
- Görev soruları doğrulanmış zinciri izler: görev → commit → değişen dosyalar → semboller.

### 6.4 Depo haritası (D-057/058, Aider yöntemi)

- Düğüm = dosya; kenar = "dosya A, dosya B'nin tanımladığı bir ismi kullanıyor". Ağırlık = √kullanım × isim ağırlığı (belirgin isim ×10, `_x` ×0.1, 5'ten fazla dosyada tanımlı ×0.1).
- PageRank (sönüm 0.85, 20 tur); tohumlar = arama isabetlerinin dosyaları (1/sıra ağırlığı), sorudaki dosya adları, belirgin isimleri tanımlayan dosyalar.
- Çıktı: en iyi dosyalar + en çok kullanılan 3 sembol imzası, bütçe içinde (`repo_map_budget` 500; proje kartında 300; SessionStart'ta 600).

### 6.5 Konum cevabı (D-062, Agentless yöntemi)

İstek metni talimat kelimelerinden temizlenir → dosyalar puanlanır (dosya başına en iyi 3 isabetin toplamı, belge ×0.3, test/örnek/bench ×0.5, harita sırası %30 pay) → ilk 5 dosya → dosya başına 3 sembol ve satır aralıkları. Cevap: "Files to change, best first: … read only those ranges." (~300 token).

### 6.6 Sıkıştırma (D-061, LLMLingua benzeri, modelsiz)

300 karakterden uzun bellek/olay/dosya metni; birimler (cümle/satır) soru terimi sayısına göre önem sırasıyla seçilir, metin sırasıyla birleştirilir, boşluklar "…" ile gösterilir; öğe başına en çok 450 karakter. Kapalıyken en çok terim içeren tek pencere kullanılır.

### 6.7 Kapsül kuralları (D-056)

- Aynı tam metin bir kez; aynı takip mesajı bir kez.
- **`capsule: false`** (istemci kapsül eklemez) yalnızca şu koşulların hepsi doğruysa: yerel cevap yok, istem ≤5 kelime, ≤3 içerik terimi, isim/ref/yol/sürüm yok, VE (istem konuşmaya işaret ediyor — "bunu", "bunların", "this", "it" — VEYA ≤2 terimli bir talimat / tam eşleşmesi olmayan kısa soru). Listeleme cevapları asla `false` almaz. API yalnızca `false` değerini gönderir.

### 6.8 Toplu expand (D-063)

`zcie_expand` / `POST /v1/expand`: en çok 20 ref; bunlardan en çok 10'u `path:start-end` satır aralığı olabilir, her biri en çok 100 satır (fazlası için `more: path:a-b`). Sembol refleri için `with: ["callers","tests"]` → kullanan dosyalardaki ilk çağrı satırları (en çok 5'er, import satırları hariç). Seviyeler: `evidence`, `source`, `audit` (yalnız admin).

---

## 7. Claude Code entegrasyonu

### 7.1 Hook'lar (`~/.claude/settings.json` içine yazılır)

| Olay | Script | Görev |
|---|---|---|
| SessionStart | `session-start.sh` | Projeyi bul/oluştur, `session.start` gönder; sıkıştırmadan sonra "görülen ref" listesini sıfırla. |
| SessionStart (startup) | `session-map.sh` | Depo haritasını bir kez yazdır (600 token). |
| SessionStart (startup) | `session-context.sh` | Kısa çalışma kuralları + kapatılan araçların kataloğu. |
| UserPromptSubmit | `user-prompt-submit.sh` | Yeni tur, `user.message` yakala. |
| UserPromptSubmit | `inject-context.sh` | Bağlam kapsülü (aşağıda). |
| UserPromptSubmit | `session-guard.sh` | Oturum ~60k token'ı geçince uyarı. |
| UserPromptSubmit | `tool-unlock.sh` | Gerektiğinde kapalı bir aracı bu oturum için açar. |
| PreToolUse (Read) | `read-guard.sh` | 400 satırdan uzun dosyanın ofsetsiz okunmasını ilk 250 satırla sınırlar, sembol taslağını (satır aralıklarıyla) verir. |
| PostToolUse | `post-tool-use.sh` | Edit/Write → `file.changed`, Bash → `command.run`. |
| Stop | `assistant-message.sh` | Asistanın son cevabını kaydeder. |
| SessionEnd | `stop.sh`, `tool-relock.sh` | `session.end`; açılan araçları tekrar kapatır. |

Kural: **hook asla oturumu engellemez veya bozmaz; her yol `exit 0` ile biter.** Ağ çağrıları `--max-time 2`.

### 7.2 Bağlam kapsülü (`inject-context.sh`)

- Sorgu: `cross_project: true`, bütçe `ZCIE_INJECT_BUDGET` (1.200).
- Eleme: bu oturumda zaten verilen ref'ler, istemin yankısı, aynı kararın olay kopyası, araç günlükleri, soru olarak saklanmış kısa kayıtlar.
- Kapı (kapsül eklenir mi?): `capsule != false` VE (yerel cevap var VEYA sınıf task/implementation/timeline/exact_ref VEYA istemde adı geçen bir sembol VEYA başka proje etiketi VEYA `match_coverage ≥ 0.6`).
- Biçim: başlık satırı → `answer:` → en iyi 6 öğe (**en iyi ilk, ikinci en iyi son** — "Lost in the Middle"; düzeltilmiş eski kayıtlar sondan bir önce) → istemde adı geçen ilk sembolün gövdesi (≤40 satır) → indeks günceliyse "The index is current: use these facts as they are, with no Grep or Read check…".
- Bellek/olay öğelerinde başlık tekrar yazılmaz, yalnızca metin; önceki mesaj ≤150, sonraki mesajlar ≤200 karakter.

### 7.3 Araç profilleri (`zcie claude setup --profile`)

- `auto` (varsayılan): araçlar talep üzerine yüklenir (`ENABLE_TOOL_SEARCH`), ağır 6 araç kapalı: Skill, Agent, Workflow, ScheduleWakeup, ReportFindings, ListAgents (~27k → 7k token/tur).
- `balanced`: 15 araç kapalı (~14k). `lean`: 21 araç kapalı (~6k). `off`.
- **Önemli:** Claude Code için MCP sunucusu kaydedilmez; varsayılan yol hook'lardır (ADR-008 I-37). `zcie_query`/`zcie_expand` MCP araçları OpenCode ve açık MCP kullanımı içindir.

### 7.4 Hook ortam değişkenleri

`ZCIE_URL` (127.0.0.1:47831), `ZCIE_API_KEY` veya `ZCIE_API_KEY_FILE` (`~/.zcie/config/claude-code.key`), `ZCIE_WORKSPACE` (geçersiz kılma), `ZCIE_INJECT_BUDGET` 1200, `ZCIE_READ_MAX_LINES` 400, `ZCIE_READ_HEAD_LINES` 250, `ZCIE_SESSION_WARN_TOKENS` 60000.

---

## 8. API ve CLI

### 8.1 HTTP API (Bearer anahtar zorunlu)

`POST /v1/events`, `POST /v1/repositories`, `POST /v1/query`, `POST /v1/expand`, `GET /v1/entities/{ref}`, `GET /v1/timeline/{ref}`, `POST /v1/tasks`, `POST /v1/links`, `POST /v1/memory/{id}/correct`, `POST /v1/forget`, `GET /v1/health`, `POST /v1/admin/rebuild-search`, `GET|POST /v1/projects`, `GET|DELETE /v1/projects/{p}`, `POST /v1/projects/{p}/rename`, `GET /v1/projects/{p}/status`.
Anahtarlar: `admin.key` (yönetici), `claude-code.key` (hook'lar). Anahtarlar asla repoya yazılmaz.

### 8.2 CLI komutları

`daemon`, `repo`, `query [--mode map|locate|…] [--global]`, `expand <ref|path:a-b>… [--with callers,tests]`, `capture`, `link`, `forget`, `health`, `doctor [--fix]`, `rebuild-search`, `claude`, `opencode`, `vector`, `import` (`import claude`), `project` (`add`, `status`, `init`, …), `setup`, `config show|set`, `verify [--live]`, `uninstall [--purge]`, `key`.

---

## 9. Kurulum (ADR-014)

- `npm install -g ./dist/zcie-0.1.0.tgz` (paket: `scripts/packaging/build-npm.sh`, şimdilik yalnız macOS Apple Silicon) → `zcie setup` sihirbazı.
- `setup.toml` ayarları: `language` (tr/en), `port` (47831), `vector` (multilingual/english/off), `workers` (1/2/4), `autostart` (launchd), `claude` (user/project/none), `claude_dir`, `profile` (auto/balanced/lean/off), `budget` (200–4000, 1200), `opencode`, `answers` (asistan cevaplarını kaydet, varsayılan açık).
- `apply` → `engine.toml` + `client.json` + hook'lar; `zcie setup --yes` mevcut ayarları koruyarak yeniden uygular.
- launchd ajanı (etiket ev klasörüne göre hash'li) `~/.zcie/bin/zcied` kopyasını çalıştırır.
- `zcie verify` 11 kontrol (programlar, curl/jq, daemon, autostart, engine.toml, hook'lar, yeni klasöre ID, yeni oturumun eski olguyu bilmesi, diller arası arama, MCP); `--live` iki gerçek `claude -p` oturumu.
- **macOS TCC:** `zcied` ad-hoc imzalı; her yeni derleme yeni kimlik alır ve Masaüstü erişim izni ister. İzleyici bu izni beklerken artık kilit tutmaz (proje silme/ekleme beklemez).

---

## 10. `engine.toml` bölümleri ve varsayılanlar

| Bölüm | Önemli alanlar (varsayılan) |
|---|---|
| `[server]` | `http_addr` 127.0.0.1:47831, `allow_remote` false |
| `[storage]` | `data_dir` ~/.zcie/instances/local, `sync_writes` true |
| `[ingest]` | `max_payload_bytes` 1 MB, `queue_capacity` 10.000, `worker_count` 4, `debounce_ms` 200 |
| `[index]` | `max_file_bytes` 1 MB, `languages` rust/typescript/python |
| `[query]` | `default_token_budget` 250, `max_token_budget` 4000, `max_results` 8, `graph_max_depth` 3, `graph_max_fanout` 50 |
| `[cache]` | `l0_capacity_entries` 10.000, `negative_ttl_ms` 2000 |
| `[security]` | `redact_secrets` true, `exclude_globs` (bkz. 4.2) |
| `[capture]` | `assistant_messages` (setup'tan), `command_output_max_bytes` 65.536 |
| `[vector]` | `enabled` true, `model` çok dilli, `min_similarity` |
| `[context]` | `capsule_rules` true, `repo_map_budget` 500 (0 = kapalı), `graph_rank` true, `compress` true |

Ortam değişkeni geçersiz kılma biçimi: `ZCIE_<BÖLÜM>_<ALAN>` (örn. `ZCIE_VECTOR_ENABLED=false`).

---

## 11. Geliştirme kural seti (zorunlu)

### 11.1 İş sırası (`docs/00-agent-rules.md`, ADR-007)

1. `docs/README.md` ve okuma sırasındaki belgeleri oku.
2. Geliştirme görevlerini `docs/12-development-plan.md` içindeki numara sırasıyla yap (D-001 … D-065).
3. **Geliştirme görevleri sırasında test yazma ve test çalıştırma.** Her görevden sonra yalnızca `cargo build --workspace` ve `cargo clippy --workspace -- -D warnings`; tüm hata ve uyarıları düzelt.
4. Görevin yalnızca "Files" alanındaki dosyalarını değiştir; "Done when" maddelerinin hepsi doğru olmalı; bitince `[ ]` → `[x]`.
5. Tüm geliştirme görevleri bitince test aşaması (`docs/13-test-plan.md`): testleri yaz, çalıştır, hepsi geçene kadar kodu düzelt.
6. Tüm testler geçince kabul ölçütlerini (`docs/14-acceptance-criteria.md`) incele ve her birinin sonucunu yaz.
7. Belirsiz bir görevde büyük tasarım tahmin etme: belgelere uyan en basit çözümü seç, kararı yeni bir ADR olarak `docs/adr/` içine yaz.

### 11.2 Kod sadeliği kuralları

| ID | Kural |
|---|---|
| C-01 | Fonksiyon ≤ 40 satır. |
| C-02 | Kaynak dosya ≤ 400 satır; büyürse modüllere böl. |
| C-03 | Kendi makronu yazma (`macro_rules!`, proc-macro); yalnız onaylı crate'lerin `derive`'ları. |
| C-04 | Tek somut tip yetiyorsa generic kullanma. |
| C-05 | Tek gerçekleştirmesi olan trait/trait object kullanma (istisna: `zcie-vector` içindeki `VectorIndex`). |
| C-06 | `unsafe` yok. |
| C-07 | `main()` ve testler dışında `unwrap()`/`expect()` yok; `Result` döndür. |
| C-08 | Her crate'te tek hata tipi (`thiserror`); uygulamalar `anyhow`. |
| C-09 | Açık isimler; sözlükteki (`docs/02-glossary.md`) terimler dışında kısaltma yok. |
| C-10 | Her public fonksiyon/struct/enum için 1–2 cümlelik `///` yorumu. |
| C-11 | Benchmark sorun göstermeden optimizasyon yapma. |
| C-12 | Yalnızca onaylı crate'ler; yeni crate için önce ADR. |
| C-13 | Planın adlandırmadığı crate oluşturma. |
| C-14 | Belgelerin tanımlamadığı özellik ekleme. |
| C-15 | Log için `tracing`; kütüphanelerde `println!` yok. |
| C-16 | `.await` boyunca kilit tutma. |
| C-17 | Sabitler (limit, port, boyut) yapılandırmada veya adlandırılmış sabitlerde; satır içi sayı yok. |

### 11.3 Onaylı crate'ler

serde, serde_json, thiserror, anyhow (yalnız apps), tokio, tracing, tracing-subscriber, rocksdb (`default-features=false`, lz4), blake3, zstd, getrandom, ulid, time, axum, reqwest (blocking+json, TLS yok), clap (derive), toml, ignore, globset, notify, tree-sitter (+rust/typescript/python), tantivy, lru, rmcp, regex, tempfile (yalnız test). Git verisi için **`git` komut satırı** (`git2`/`gix` yasak, ADR-004). Rust edition 2024, `resolver = "3"`, stable toolchain.

### 11.4 Belge dili (STE-100 tabanlı, ASD-STE100)

Belgeler ve kod yorumları kontrollü, basit teknik İngilizce (Sonnet sınıfı bir modelin okuyacağı varsayılır):
W-01 prosedür cümlesi ≤20 kelime · W-02 açıklama cümlesi ≤25 kelime · W-03 cümle başına tek talimat · W-04 talimatta emir kipi · W-05 etken çatı · W-06 tek anlamlı kelimeler, sözlük terimleri · W-07 "-ing" isim/sıfat yerine alternatif · W-08 artikeller · W-09 paragraf ≤6 cümle · W-10 uyarı, ilgili talimattan önce · W-11 zorunluluk SHALL/MUST, seçenek MAY, yasak SHALL NOT · W-12 adımlar numaralı, sırasız öğeler madde işaretli.

### 11.5 Durup sorma koşulları

Onaysız crate gerekirse; bir görev başka bir belgeyle çelişirse; derleme hatası 3 denemede düzelmezse; crate indirme dışında uzak servise ağ erişimi gerekirse.

### 11.6 Kullanıcının çalışma tercihleri

- Kullanıcıya **Türkçe** cevap ver.
- Çok adımlı işlerde ilerleme çubuğu göster: 25 hücre (█/░), iki boşluk, yüzde 3 karaktere sağa yaslı + `%`, üç boşluk, görev etiketi. Yüzde sayılan öğelerden hesaplanır (dolu hücre = round(yüzde/4)), payda söylenir, sayı uydurulmaz.
- "Bana soru sorma, best practice seç." Kararı kendin ver, gerekçesini yaz.
- "Testleri sen yap": test ve benchmark'ları ajan çalıştırır.
- **Yalnızca kullanıcı isterse commit et.** Son commit `745a732`; ADR-010…018 ve P4 commit edilmedi. Commit mesajı sonuna: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

### 11.7 Güvenlik kuralları (ihlal edilemez)

- API anahtarları asla repoya yazılmaz; yalnızca `~/.zcie/config/*.key` (izin 0600).
- Hook'ları asla `sh -x` ile izleme (anahtarı sızdırır).
- Ham konuşma kayıtları ve yakalanmış istekler kullanıcının CLAUDE.md'sini ve hesap ID'lerini içerir; repoya kopyalanmaz.
- Model kendi araç izinlerini değiştirmemelidir.
- `.gitignore`: `.claude/settings.local.json`, `zcie-setup.json`, `.env`, `/dist/`.
- Benchmark/test oturumlarında dış ortam değişkenleri (`ZCIE_WORKSPACE`, `ZCIE_URL`, `ZCIE_API_KEY*`, `ZCIE_SETTINGS_FILE`) temizlenir.

---

## 12. Test ve ölçüm altyapısı

- Testler: `tests/integration/*.rs` (TS-01 … TS-20), `tests/support` (TestEngine, DaemonProcess, örnek depolar `fixture_repo`, `ledger_repo`), `tests/token-regression` (yanıt boyutu taban çizgisi, +%10 sınır). Bugün **181/181 geçiyor**, clippy temiz.
- Benchmark'lar (`benchmarks/`):
  - `claude-ab/run_ab.py` + `context/run.py`: gerçek `claude -p` oturumları; koşul **U** = kurulu ZCIE, **A0** = hook'suz. Görev setleri `context/code.json` (27), `context/locate.json` (20).
  - `memory-30/simulate.py` (30 aşamalı hafıza, modelsiz), `user-ideas/analyze_ui.py` (15 soru), `user-ideas/noise.py` (gündelik/genel istemler), `context/offline_locate.py`, `vector/eval.py`, `end-to-end/bench.py` (gecikme), `rerank/spike.py`.
  - Uyarı: U koşulu kendi istemlerini benchmark projelerinin hafızasına yazar; ardışık koşularda hafıza gürültüsü artar.

---

## 13. Bugünkü durum (2026-10-17)

- Geliştirme planı D-001 … D-065 tamam. Son faz **P4 (bağlam verimliliği, ADR-017)**: kapsül kuralları, dosya referansları, depo haritası, bağlam metni, grafik PPR, sıkıştırma, konum modu, toplu expand, yeniden sıralayıcı denemesi (eklenmedi, ADR-018), sonuç ölçümü.
- Kabul: **34/36 geçti.**

| Ölçüt | Sonuç |
|---|---|
| AC-34 hafıza | 17/17 ve 15/15; kapsül 216→159 (−%26), 720→521 (−%28) token ✓ |
| AC-35 gürültü + gecikme | genel 0/15, gündelik 0/15; p95 2 ms (sentetik), tantivy'de 97,5 ms ✓ |
| AC-36 konum | dosya arama 1,53→0,60, token −%32, 38/40 ✓ (sınırda) |
| AC-32 kod tokeni | 16.435→15.561 (−%5) ✗ (hedef −%30; düzenleme görevleri Read+Edit zorunlu, alt sınır ≈12.300) |
| AC-33 kod araması | 0,65→0,48 (−%26) ✗ (hedef −%40) |

- Açık işler / bilinen sınırlar: AC-32/33 için karar (kabul veya hedef değişikliği); "açıkla" türü görevlerde kapsülün eksikliği (T7 hâlâ dosya okuyor); büyük depoda konum gecikmesi (iki arama yerine bir); `CALLS` kenarları üretilmiyor; kararlı kod imzası (macOS izni her derlemede); npm paketi yalnız darwin-arm64.

## 14. Kararlar listesi (ADR)

001 Rust çekirdek · 002 JSON değerler · 003 tek yazıcı daemon · 004 git CLI · 005 türetilmiş arama indeksi · 006 iki MCP aracı · 007 testler geliştirmeden sonra · 008 uygulama kararları (I-01…) · 009 merkezi proje hafızası (`zcie_id`) · 010 tek turda cevap ve operasyon · 011 yerel vektör arama · 012 SDK'lar ve OpenCode · 013 hafıza doğruluğu (Türkçe, pasajlar, düzeltmeler) · 014 npm kurulum ve sihirbaz · 015 yalnız eklenen klasörler indekslenir · 016 proje kartı, asistan cevapları, dosya içerikleri · 017 bağlam verimliliği · 018 yerel yeniden sıralayıcı reddedildi.

---

## 15. Güncelleme: P5.1 ve P5.2 (2026-10-17, ADR-019)

- **Sorgu izi:** `zcie query --explain` (istekte `explain: true`) her adayın neden tutulduğunu/düştüğünü (`kept`, `echo`, `duplicate`, `below_cut`) ve aşama sürelerini verir. İz loga yazılmaz.
- **Yalıtılmış benchmark:** `benchmarks/isolated` — dondurulmuş ana dizin (`snapshot.py`), her denemede APFS klonu + kendi daemon'u, koşullar A0/A0P/U/UP, tohumla rastgele sıra, `report.py` ile doğru koşu başına token ve eşli farklar (%95 bootstrap). Kullanıcının canlı hafızası kirletilmez.
- **Niyet:** yanıtta `intent` = `explain`, `locate`, `edit`, `trace`, `memory`, `general` (modelsiz, sabit ifadeler + sorgu sınıfı).
- **Kanıt kesitleri:** `edit` (≤3 sembol), `explain`/`trace` (≤5 sembol) için `evidence[]`: yol, satır aralığı, içerik hash'i, değiştirilmemiş kaynak metni, `requires_live_read`. Metin saklanan kopyadan gelir (sorgu diski okumaz); bekleyen değişiklik varsa veya klasör izlenmiyorsa `requires_live_read: true`. Test fonksiyonları (soru test istemiyorsa) atlanır. Kaynak kodu asla sıkıştırılmaz.
- **Hook:** kesitleri "current/changed" etiketiyle basar; aynı revizyon oturumda bir kez gönderilir; oturum çalışma kümesi (`zcie-<oturum>.work`, metin sağlama toplamıyla) sayesinde "bunu düzelt" gibi istemlerde yalnızca değişen kesitler yeniden verilir. P4'teki koşulsuz "Grep/Read yapma" cümlesi kaldırıldı.
- **Vektör doldurma:** 512'lik gruplarla tek kilit (sorgu sırasında p95 ~100 ms).
- **Sonuç (A0'a göre, U / UP):** kod −%51 / −%73 token; konum −%28 / −%68; düzenleme (doğru koşu başına) 49,5k / 24,9k karşı 61,2k. Kabul 37/45.
- **Sırada:** P5.3 (sembol kimliği, CALLS kenarları, dosya bazlı tazelik), P5.4 (hafıza kaynağı, kapsam, eksiksiz silme), P5.5 (vektör anlık görüntüsü, önbellek kimliği, araç çıktısı kısaltma).

## 16. Güncelleme: P5.3–P5.6 ve doğruluk garantileri (2026-10-18, ADR-020, ADR-021)

- **Kullanıcı kuralı:** ZCIE açıkken cevaplar ve yazılan kod %100 doğru olmalı. ZCIE'nin garantisi: (1) verdiği bilgi doğru ve etiketli (doğrulanmamış / asistan cümlesi / düzeltilmiş açıkça yazılır), (2) kod değişikliği doğrulama kapısından geçmeden tur bitmez, (3) sabit test setleri her tekrarda %100.
- **Doğrulama kapısı (`verify-gate.sh`):** PostToolUse (Edit/Write/…/Bash) değişen dosyaları kaydeder; Stop'ta en yakın `Cargo.toml` → `cargo check`, `tsconfig.json` → `tsc --noEmit`, Python → `py_compile` (veya `.claude/zcie_verify` komutu). Hata varsa `decision: block` + ilk 60 satır; tur başına en çok 3 kez. Bash değişiklikleri Git üzerinden bulunur. Ayar: `check` (auto/off). Claude Code zaman sınırı 150 sn.
- **Çağıranlar:** her dosyanın çağrı noktaları saklanır (`file_calls`, ayrıştırıcı v4); sorgu anında aynı dosya / import / nitelikli yol ile "verified", tek tanım "inferred", birden çok "candidate". Başka tanıma giden çağrı çağıran sayılmaz.
- **Hafıza kaynağı:** asistan cümleleri `assistant_statement` olarak etiketlenir ve kullanıcı bilgilerinin altında sıralanır; düzeltilmiş kaydın durumu hiçbir yol tarafından ezilmez.
- **Silme:** unutulan mesajdan doğan görevler de silinir; yeniden başlatmadan sonra hiçbir indeks kaydı döndürmez.
- **Önbellek:** anahtar, projenin dosya değişikliği sayacını (`code_seq`) içerir; editörde yapılan değişiklik eski yanıtı geçersiz kılar.
- **Kapsam:** hook anahtarı yönetici işlemlerini yapamaz; satır aralığı proje dışına çıkamaz; ayar `cross_project`.
- **Hook zaman sınırları:** kapsül sorgusu 4 sn bekler (hook sınırı 10 sn); zaman aşımında görünür not basılır.
- **Sonuç:** yalıtılmış benchmark, 3 tekrar: U ve UP 141/141 doğru; 27 kod görevi ZCIE'siz oturuma göre −%51 / −%73 token. Kabul 45/47 (AC-32/33 yerine AC-45/44).
