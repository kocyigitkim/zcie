# zcie — ZeroContext Intelligence Engine

Local project memory and code index for Claude Code (and OpenCode). Each new session gets the facts, decisions and ideas of your earlier sessions, without a model call and without network access at query time.

## Kurulum (Türkçe)

```sh
npm install -g ./zcie-0.1.0.tgz   # paket dosyasından (./ önemli); kayıt defterinden: npm install -g zcie
zcie setup                 # sihirbaz: ayarları sorar, kurar ve doğrular
```

Sihirbaz şunları sorar ve hemen uygular:

| Ayar | Seçenekler | Varsayılan |
|---|---|---|
| Dil | Türkçe, English | sistem dili |
| Claude Code | tüm projeler, bir klasör, bağlanmasın | tüm projeler |
| Araç profili | auto, balanced, lean, off | auto |
| Bağlam bütçesi (token) | 200–4000 | 1200 |
| Anlamsal arama | çok dilli, İngilizce, kapalı | çok dilli |
| Arka plan indeksleme | 1, 2, 4 işçi | 2 |
| Oturum açılışında başlat | evet, hayır | evet (macOS) |
| Port | 1024–65535 | 47831 |
| OpenCode | evet, hayır | kuruluysa sorulur |

Sonunda eski Claude Code sohbetlerinizi hafızaya aktarmayı ve iki gerçek Claude Code oturumuyla canlı test yapmayı önerir.

Bir projenin kodunu indekslemek ve değişikliklerini izlemek için klasörü ekleyin (yalnızca eklenen klasörler indekslenir):

```sh
zcie project add ~/Desktop/proje       # ekler, indeksler, bitene kadar ilerlemeyi gösterir
zcie project status ~/Desktop/proje    # dosya, sembol, commit, bekleyen iş
```

Elle sorgu (hook'lar bunu her istemde otomatik yapar):

```sh
zcie query "gecikme faizi nerede hesaplanıyor"            # kanıtlı sonuçlar
zcie query --mode locate "late fee'ye vergi ekle"         # değişecek dosyalar ve satır aralıkları
zcie query --mode map "genel bakış"                      # depo haritası: önemli dosyalar ve sembolleri
zcie expand src/fees.rs:1-40 src/invoice.rs:5-20 --level source   # tek çağrıda birden çok satır aralığı
```

Ayarları sonradan değiştirmek:

```sh
zcie config show                       # tüm ayarlar
zcie config set budget 1500            # bir ayar; hemen uygulanır ve doğrulanır
zcie config set vector english workers 1
zcie setup                             # sihirbaz, mevcut değerler varsayılan
```

Çalıştığını doğrulamak:

```sh
zcie verify          # programlar, daemon, hook'lar, yeni oturumda hatırlama, diller arası arama, MCP
zcie verify --live   # ek olarak iki gerçek Claude Code oturumu (yaklaşık 0,05 $)
zcie doctor          # kuyruk, indeks ve proje durumu
```

Kaldırmak:

```sh
zcie uninstall           # Claude Code/OpenCode girdileri, otomatik başlatma, daemon; hafıza kalır
zcie uninstall --purge   # hafızayı da siler
npm uninstall -g zcie
```

Etkileşimsiz kurulum (betikler için): `zcie setup --yes --language tr --claude user --vector multilingual --workers 2`.

## Install (English)

```sh
npm install -g zcie
zcie setup            # wizard: asks the settings, applies them, verifies the installation
zcie config set <key> <value>   # change a setting later
zcie verify [--live]  # end-to-end check
zcie query --mode locate "add a tax to the late fee"   # files and line ranges to change
zcie query --mode map "overview"                       # repository map
zcie expand src/fees.rs:1-40 src/invoice.rs:5-20       # several line ranges in one call
```

Requirements: macOS on Apple Silicon (this package), `curl` and `jq` (macOS 15+ has both; else `brew install jq`), Node.js 18+ for the launcher. Data: `~/.zcie` (`ZCIE_HOME` changes it).
