# Echo review | `vdl rename` v2

Status: DONE_WITH_CONCERNS  
Gate verdict: FAIL  
Branch/worktree: `feat/split-entities`, worktree utama  
Decision owner / handoff: controller

## Findings

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Critical | R2, R5 | `src/rename.rs:234`, `src/rename.rs:304`, `src/cli/mod.rs:388` | Simulasi mengganti ID model langsung dengan `new`, tetapi deklarasi ditulis sebagai `local_id(new)` tanpa membuktikan hasil parse. Contoh deduksi: `package p` dengan entitas `Old` tanpa bare ref, `p.Old -> q.New` lolos simulasi, menghasilkan `p.New` di disk dan baru gagal sesudah write. Nama baru yang hanya valid saat dikutip juga dapat lolos simulasi dan gagal parse setelah write. |
| Critical | R3 | `src/rename.rs:378`, `src/rename.rs:404`, `src/cli/mod.rs:388` | Transformasi berjalan pada setiap komponen setiap source, termasuk project root dan ancestor. Rename `p.old` memindahkan juga file `q/old.vdl` untuk entitas lain. Bila root bernama `old`, operasi dapat memindahkan keseluruhan project dan CLI kemudian mencoba membuka path lama. Kontrak `<file_or_dir>` juga rusak ketika file input tunggal dipindah lalu path lama dipakai saat reload. |
| Important | R5 | `src/rename.rs:240`, `src/rename.rs:262`, `src/project/loader.rs:213` | Guard hanya memeriksa bare ref yang tadinya menuju `old`. Contoh deduksi: file dalam `p` punya bare `next` teresolusi ke `q.next` melalui `import q.*`. Rename `p.old -> p.next` tanpa referensi lama dapat membuat `next` diam-diam menunjuk `p.next` dan `validate` tetap hijau. Explicit import atas `old` juga dibiarkan, sehingga project bisa gagal reload setelah write. |
| Important | R1, R5 | `src/rename.rs:564`, `src/rename.rs:586`, `src/model/mod.rs:276` | Resolver mendukung quoted full-ID include, tetapi cabang `!id.contains('.')` melewati `include "p.old"`. Untuk entitas dengan local ID `include`, `line_end.find(id)` memilih kata directive pertama pada `include include`, sehingga keyword ikut berubah. Scanner raw juga memakai aturan indentasi yang lebih longgar dibanding scanner Markdown model. |
| Minor | Scope | `src/cli/mod.rs:16` | Diff masih memformat fungsi-fungsi CLI di luar `cmd_rename`, termasuk panggilan jalur compile/compose. |

## Bukti yang diperiksa

| SHA | Perubahan | File | Test/evidence |
| --- | --- | --- | --- |
| N/A | Review read-only | `C:/Users/khayren/voidlight/vld-compiler/src/rename.rs`, `src/cli/mod.rs`, `tests/rename_test.rs`, `tests/fixtures/rename-v2/`, `docs/cli.md`, `docs/plans/rename-v2-report.md` | Inspeksi `git diff`, `git diff --name-only`, source dengan nomor baris, fixture dan catatan run controller. Tidak ada test yang dijalankan ulang oleh reviewer. |

R4 terbukti untuk fixture saat ini: output menunjukkan pasangan spelling, path file dan folder, dua pergeseran urutan, dan 7 edit span. Controller menunjukkan full suite dan build release sukses serta demo tervalidasi dengan 4 entitas. Bukti tersebut mencakup jalur utama fixture, sedangkan temuan di atas berasal dari inspeksi alur dan batas input. Report implementasi yang terbaca saat awal review masih berupa plan.

## Urutan perbaikan

1. Validasi deklarasi dan resolusi seluruh referensi dari bytes yang akan ditulis, termasuk import dan include, sebelum mutasi.
2. Batasi pergerakan path ke file pemilik entitas dan folder terkait di bawah root. Reload dari path final untuk input file tunggal.
3. Gunakan aturan directive model untuk memilih include yang aktif, pertahankan bentuk quoted, dan ambil offset operand setelah keyword.

Verdict: fixture utama berjalan, tetapi gate gagal karena ada rename yang merusak atau mengubah resolusi project setelah `--write`.

## Re-review 2 (2026-11-28) - delta since first review

Status: DONE_WITH_CONCERNS  
Gate verdict: FAIL  
Branch/worktree: feat/split-entities, worktree utama  
Decision owner / handoff: controller

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Critical | R2, R5 | src/rename.rs:219, src/rename.rs:256, src/rename.rs:400, src/lexer/mod.rs:404 | Guard memeriksa namespace tetapi belum memeriksa apakah nama lokal valid sebagai bare identifier. Rename p.old ke p.-new dapat lolos simulasi lalu mengganti deklarasi dengan -new, yang ditolak lexer saat reload. |
| Critical | R5 | src/rename.rs:423, src/project/loader.rs:206 | Guard explicit import berjalan pada entitas saja. Import p.old dalam file schema atau file tanpa entitas tetap tercatat di doc.imports dan diverifikasi saat reload. Rename dapat menulis perubahan sebelum loader menolak import tersebut. |
| Important | R1, R5 | src/rename.rs:614, src/rename.rs:685, src/model/mod.rs:245 | Scanner rename menerima indentasi yang diabaikan scanner model, sehingga contoh kode Markdown yang menulis include p.old ikut berubah. Bare include aktif juga tidak dicek terhadap model baru. Jika include next sebelumnya memilih q.next melalui wildcard, rename p.old ke p.next bisa membuatnya memilih p.next melalui paket lokal. |
| Important | R3, R5 | src/rename.rs:540, src/rename.rs:558 | Case fold hanya dibandingkan dengan path VDL lengkap. Pada filesystem case sensitive, folder next dapat dibuat di samping NEXT yang kosong atau berisi file bernama lain. Nested move juga memeriksa tujuan akhir di disk sebelum folder induk berpindah, sehingga bisa melewatkan tujuan yang sudah ada di namespace asal. |
| Minor | Scope | src/cli/mod.rs:16, src/cli/mod.rs:136 | Diff masih memuat perubahan format CLI di luar cmd_rename. |

File pemilik dan folder terkait kini dibatasi di bawah project root. File senama di luar folder target tetap di tempat. Reload input tunggal memakai path baru. Guard namespace menolak perpindahan lintas package, dan bare reference yang tidak diedit turut dicek terhadap kandidat baru. Quoted include dan offset argumen telah diperbaiki. R4 didukung output fixture yang menunjukkan delapan edit, perubahan path, dan posisi kedua child. Fixture memakai field body biasa sehingga belum membuktikan bahwa include aktif tetap mengarah ke entity yang benar saat compose.

| SHA | Perubahan | File | Test/evidence |
| --- | --- | --- | --- |
| N/A | Re-review read-only | C:/Users/khayren/voidlight/vld-compiler/src/rename.rs, C:/Users/khayren/voidlight/vld-compiler/src/cli/mod.rs, C:/Users/khayren/voidlight/vld-compiler/tests/rename_test.rs, C:/Users/khayren/voidlight/vld-compiler/tests/fixtures/rename-v2/ | Inspeksi git diff, implementasi, fixture, serta catatan 286 tes lulus, release build dan validasi demo dengan 5 entitas. Tes tidak diulang dalam review read-only. |

Urutan perbaikan:

1. Validasi bare spelling melalui lexer dan cek explicit import pada doc.imports sebelum menulis.
2. Gunakan scanner model dan cek kandidat setiap include aktif sebelum dan sesudah rename.
3. Inspeksi sibling tujuan secara case insensitive pada namespace asal sebelum memindahkan folder induk.

Verdict: Jalur demo lulus, tetapi rename masih bisa menulis deklarasi yang gagal parse, meninggalkan import rusak, atau mengubah resolusi include.

## Re-review 3 (2026-11-28) - delta since review 2

Status: DONE_WITH_CONCERNS  
Gate verdict: FAIL  
Branch/worktree: feat/split-entities, worktree utama  
Decision owner / handoff: controller

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Critical | R1, R5 | src/rename.rs:609, src/rename.rs:865, src/lexer/mod.rs:280, src/parser/schema.rs:49 | Indentasi scanner source berhenti pada setiap baris yang setelah trim berisi `}`, walaupun lexer hanya mengakhiri body pada delimiter yang sejajar dengan field. Letakkan baris Markdown `}` pada indentasi 8 sebelum `include p.old` pada indentasi 8, sedangkan penutup field berindentasi 4. Parser mempertahankan baris Markdown dan dedent mengaktifkan include. Scanner source mendapat indentasi dasar 0 dan melewatkan include. Simulasi mengubah include pada model sehingga dry-run dapat lolos, lalu reload gagal setelah write karena sumber masih menunjuk ID lama. |
| Important | R1, R5 | src/rename.rs:568, src/rename.rs:882, src/model/mod.rs:276 | Guard memakai resolver, tetapi pengumpul edit langsung memakai perbandingan literal. Jika ada entitas `old` global dan `p.old`, maka `include old` di package `p` menunjuk `p.old`. Rename entitas global `old` menjadi `new` tetap mengedit operand itu. Guard mensimulasikan operand tetap sehingga tidak menangkap kesalahan. |

Perbaikan sebelumnya untuk deklarasi lokal yang gagal lex, import dari file tanpa entitas, quoted include, pemeriksaan collision sibling dan nested, reload file tunggal, dan scope perubahan CLI terlihat di diff. Fixture aktif dan laporan controller mencatat 287 tes lulus, release build, 8 edit span demo, dua pergeseran posisi, validasi 5 entitas, dan ekspansi include setelah rename.

| SHA | Perubahan | File | Test/evidence |
| --- | --- | --- | --- |
| N/A | Re-review read-only | C:/Users/khayren/voidlight/vld-compiler/src/rename.rs, C:/Users/khayren/voidlight/vld-compiler/src/cli/mod.rs, C:/Users/khayren/voidlight/vld-compiler/docs/plans/rename-v2-report.md | Inspeksi diff, parser, lexer, scanner, dan bukti run controller. Kedua kasus di atas diturunkan dari kode. |

Verdict: Fixture utama lulus, tetapi scanner source masih dapat melewatkan atau mengubah target include karena memakai logika berbeda dari model.

## Re-review 4 (2026-11-28) - delta since review 3

Status: DONE_WITH_CONCERNS
Gate verdict: FAIL
Branch/worktree: feat/split-entities, worktree utama
Decision owner / handoff: controller

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Critical | R2, R5 | src/rename.rs:471, src/parser/schema.rs:148 | Pemeriksaan nama lokal menerima satu token lexer. Parser juga melarang tanda hubung di akhir segmen. Rename p.old ke p.new- dapat lolos dry-run lalu menulis deklarasi yang gagal diparse saat reload. |
| Important | R1, R5 | src/rename.rs:822, src/rename.rs:833, src/lexer/mod.rs:240 | Lexer menormalkan CR tunggal menjadi newline, sedangkan pemetaan offset mentah menghitung LF saja. Dalam body dengan teks lalu CR tunggal sebelum include dan LF sebelum penutup, model melihat include aktif namun pemetaan bisa melewatkan edit. Simulasi dapat lolos dan reload kemudian gagal. |

Dua temuan review 3 tertutup untuk kasus yang dilaporkan. Pemilihan directive memakai scanner model setelah dedent, dan pengumpul memeriksa hasil resolver untuk operand dengan ejaan yang sama. Fixture menguji brace Markdown dan tes baru menguji benturan nama global dengan nama lokal. Bukti fixture juga mencakup bentuk bare, dotted, dan quoted, deklarasi lokal, pergerakan file dan folder, laporan posisi dan jumlah edit, validasi, serta invariansi jumlah entitas. Pemeriksaan git diff menunjukkan kode dan dokumentasi yang berubah berada dalam cakupan rename.

| SHA | Perubahan | File | Test/evidence |
| --- | --- | --- | --- |
| N/A | Re-review read-only | C:/Users/khayren/voidlight/vld-compiler/src/rename.rs, C:/Users/khayren/voidlight/vld-compiler/src/cli/mod.rs, C:/Users/khayren/voidlight/vld-compiler/tests/rename_test.rs, C:/Users/khayren/voidlight/vld-compiler/docs/plans/rename-v2-report.md | Inspeksi source, scanner, parser, dan git diff. Controller mencatat 288 tes lulus, release build, dan demo dry-run/write/validate/compose/stats. Tes tidak dijalankan ulang. |

Verdict: Jalur demo dan regresi review 3 tercakup, tetapi dua input yang diterima planner masih dapat menyebabkan reload gagal sesudah penulisan.

## Re-review 5 (2026-11-28) - delta since review 4

Status: DONE_WITH_CONCERNS
Gate verdict: FAIL
Branch/worktree: feat/split-entities, worktree utama
Decision owner / handoff: controller

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Important | R2, kompatibilitas | src/rename.rs:262, src/rename.rs:483, src/parser/schema.rs:412, docs/cli.md:126 | Sumber tanpa header package dapat mendeklarasikan ID absolut yang mengandung titik. Guard baru menolak rename seperti `old.id -> new.id` di sumber tersebut, walaupun CLI mendokumentasikannya dan parser bisa mempertahankan ID penuh. R2 meminta deklarasi lokal untuk file dalam package. |
| Important | R3, R4 | src/rename.rs:650, src/rename.rs:676, tests/rename_test.rs:225 | Daftar folder kandidat hanya berasal dari ancestor file sumber. Pada layout `old.vdl` yang mendeklarasikan container `p.old` dan folder sejajar `old/` yang menyimpan anaknya, file berpindah tetapi folder container tidak ikut. Dry-run juga melewatkan perubahan urutan yang dapat timbul dari perpindahan folder itu. Fixture saat ini meliputi pola folder ancestor dengan file entity di dalamnya. |

Kedua temuan review 4 teratasi. Guard mencocokkan batas tanda hubung pada segmen dengan parser. Pemetaan byte kini menghitung CRLF sebagai satu batas baris dan menangani LF serta CR tunggal, dengan regresi untuk CR tunggal. Bukti fixture meliputi semua bentuk referensi, include aktif, deklarasi lokal dalam package, pergerakan file dan folder ancestor, laporan posisi dan edit, serta invariansi jumlah entitas. Diff substansial berada dalam cakupan rename.

| SHA | Perubahan | File | Test/evidence |
| --- | --- | --- | --- |
| N/A | Re-review read-only | C:/Users/khayren/voidlight/vld-compiler/src/rename.rs, C:/Users/khayren/voidlight/vld-compiler/src/cli/mod.rs, C:/Users/khayren/voidlight/vld-compiler/tests/rename_test.rs, C:/Users/khayren/voidlight/vld-compiler/docs/plans/rename-v2-report.md | Inspeksi diff, parser, rencana path, dan bukti controller berupa 289 tes lulus, build release, dry-run, write, validate, compose, dan stats. |

Verdict: Dua regresi review 4 tertutup, tetapi gate gagal karena rename ID dotted tanpa package terblokir dan folder container sejajar terlewat.

## Re-review 6 (2026-11-28) - final gate

Status: DONE_WITH_CONCERNS  
Gate verdict: FAIL

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Important | R3, R4 | src/rename.rs:696, src/rename.rs:126, src/rename.rs:817 | Jika `old/old.vdl` mendeklarasikan `p.old` dan `old/old/child.vdl` berisi anaknya, planner memetakan folder induk ke `new/`, tetapi memakai parent lama untuk tujuan folder sibling, yaitu `old/new/`. Apply memindahkan induk terlebih dahulu lalu membuat kembali `old/` dan memindahkan anak ke sana. Path yang seharusnya adalah `new/new/child.vdl`. Dry-run menghitung posisi berdasarkan path keliru itu. |

Inspeksi independen atas diff dan dua tes regresi baru mengonfirmasi ID dotted tanpa package dapat direname dan folder sibling sederhana ikut dipindah. Kombinasi sibling dan ancestor di atas diturunkan dari planner dan urutan apply. Bukti eksekusi controller mencatat 291 tes lulus, build release lulus, 8 edit span pada demo, dua posisi bergeser, validasi 6 file dengan 5 entitas, serta ekspansi include sesudah write. Echo membaca bukti tersebut tanpa mengulang suite.

1. Petakan tujuan folder sibling melalui tujuan ancestor terdekat sebelum mengganti nama komponen terakhir.
2. Tambahkan regresi yang memeriksa penempatan akhir anak dan dry-run untuk kombinasi ancestor dan sibling.
3. Jalankan ulang verifikasi pada revisi dan minta review baru.

Verdict: Dua kasus sederhana dari review 5 tertutup, tetapi kombinasi keduanya masih dapat memisahkan folder anak dari induknya.

## Re-review 7 (2026-11-28) - final gate

Status: DONE  
Gate verdict: PASS

| Severity | Requirement | Lokasi | Bukti dan dampak |
| --- | --- | --- | --- |
| Minor | Portabilitas tes | tests/rename_test.rs:621 | Assertion awal memakai separator Windows untuk output path. Pada Linux dan macOS, assertion perlu memakai path dari `Path::join` atau menormalisasi separator sebelum membandingkan. |

Inspeksi independen mengonfirmasi planner memetakan folder sibling lewat tujuan ancestor terdalam sebelum mengganti basename. Untuk `old/old/child.vdl`, planner dan apply sekarang memakai `new/new/child.vdl`. Regresi memeriksa tujuan dry-run, kedua path akhir, hilangnya path lama, validasi, dan jumlah dua entitas.

Bukti controller mencatat 292 tes lulus, termasuk 17 tes rename, serta build release lulus. Demo release menunjukkan bentuk bare, dotted, dan quoted tetap, deklarasi lokal, delapan edit span, perpindahan file dan folder, dua pergeseran posisi, serta lima entitas setelah write dan validasi. Compose juga mengonfirmasi include aktif masih memilih entitas yang sama. Inspeksi diff membatasi perubahan kode pada rename dan handler CLI. Echo memeriksa bukti tanpa menjalankan ulang suite.

Verdict: R1–R5 lulus dan folder anak kini mengikuti perpindahan folder induk.

## Re-review 8 (2026-11-28) - final gate

Status: DONE  
Gate verdict: PASS  
Temuan terbuka: tidak ada.

Inspeksi independen mengonfirmasi bahwa assertion pada tests/rename_test.rs:621 membangun kedua path dengan `Path::join` dan `display()`, lalu membandingkan seluruh pasangan asal dan tujuan. Temuan portabilitas review 7 tertutup. Controller menjalankan regresi tersebut dan suite penuh, dengan 292 tes lulus di `.cargo-target`. Bukti build release dan demo pada review 7 tetap berlaku karena revisi terakhir hanya menyentuh assertion tes. Pemeriksaan `git diff --check` tidak melaporkan kesalahan whitespace.

Verdict: R1–R5 lulus tanpa temuan terbuka.
