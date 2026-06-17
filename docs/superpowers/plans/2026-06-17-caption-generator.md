# Caption Generator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate dua versi caption siap-copas (TikTok pendek + Instagram panjang) dari konten klip menggunakan LLM yang sudah ada, ditampilkan otomatis di halaman hasil klip.

**Architecture:** Tambah `build_caption_prompt()` di `analyze.py` (task baru `caption`), satu Tauri command `generate_caption` di `commands.rs` yang memanggil LLM via `run_llm_prompt` yang sudah ada, lalu panel caption baru di `ClipResults.tsx` yang auto-trigger saat mount.

**Tech Stack:** Python (analyze.py), Rust/Tauri (commands.rs + lib.rs), React/TypeScript (ClipResults.tsx, types.ts, i18n.tsx, App.tsx)

## Global Constraints

- Tidak ada dependency baru — gunakan `run_llm_prompt` dan `extract_json_object` yang sudah ada di commands.rs
- Bahasa caption: `id` → "Bahasa Indonesia", `en` → "English", lainnya → "English"
- LLM max_tokens untuk caption: 1024
- Format JSON output LLM: `{ caption_short, caption_long, hashtags_short, hashtags_long }`
- Copy clipboard: `caption + "\n\n" + hashtags.join(" ")`

---

### Task 1: `analyze.py` — tambah caption prompt dan task mode

**Files:**
- Modify: `scripts/analyze.py`

**Interfaces:**
- Produces: task baru `"caption"`, arg `--language`, fungsi `build_caption_prompt(segments, language) -> str`

- [ ] **Step 1: Tambah `build_caption_prompt` setelah `build_classify_prompt`**

```python
def build_caption_prompt(segments: list, language: str) -> str:
    text = "\n".join(f"[{s['index']}] {s['start_time']}: {s['text']}" for s in segments)
    lang_map = {"id": "Bahasa Indonesia", "en": "English"}
    lang_name = lang_map.get(language, "English")
    return (
        f"Kamu adalah copywriter media sosial profesional.\n"
        f"Buat caption untuk video berikut berdasarkan transkripnya.\n\n"
        f"Bahasa caption: {lang_name}\n\n"
        f"Transkrip klip:\n{text}\n\n"
        "Buat dua versi caption:\n"
        "1. TikTok (pendek): hook 1 kalimat kuat, 1-2 kalimat isi, 5-7 hashtag relevan\n"
        "2. Instagram (panjang): hook, 2-3 paragraf isi, call-to-action, 15-20 hashtag\n\n"
        "Balas HANYA dengan JSON valid tanpa teks lain:\n"
        '{"caption_short":"...","caption_long":"...","hashtags_short":["#tag1"],"hashtags_long":["#tag1","#tag2"]}'
    )
```

- [ ] **Step 2: Tambah `"caption"` ke choices dan `--language` arg**

Ganti baris argparse:
```python
parser.add_argument("task", choices=["analyze", "classify"])
parser.add_argument("--segments-file", required=True, help="Path to JSON file with segments")
parser.add_argument("--model-path",    default="",   help="Path to local .gguf model file")
parser.add_argument("--ollama-url",    default="http://localhost:11434")
parser.add_argument("--ollama-model",  default="gemma4:latest")
```
Menjadi:
```python
parser.add_argument("task", choices=["analyze", "classify", "caption"])
parser.add_argument("--segments-file", required=True, help="Path to JSON file with segments")
parser.add_argument("--model-path",    default="",   help="Path to local .gguf model file")
parser.add_argument("--ollama-url",    default="http://localhost:11434")
parser.add_argument("--ollama-model",  default="gemma4:latest")
parser.add_argument("--language",      default="id", help="Language code: id or en")
```

- [ ] **Step 3: Tambah dispatch untuk task `caption`**

Ganti baris:
```python
prompt = build_analyze_prompt(segments) if args.task == "analyze" \
         else build_classify_prompt(segments)
```
Menjadi:
```python
if args.task == "analyze":
    prompt = build_analyze_prompt(segments)
elif args.task == "classify":
    prompt = build_classify_prompt(segments)
else:
    prompt = build_caption_prompt(segments, args.language)
```

- [ ] **Step 4: Verifikasi syntax**

```bash
python3 -c "import ast; ast.parse(open('scripts/analyze.py').read()); print('OK')"
```
Expected: `OK`

- [ ] **Step 5: Commit**

```bash
git add scripts/analyze.py
git commit -m "feat(analyze): tambah build_caption_prompt dan task caption"
```

---

### Task 2: `commands.rs` — CaptionResult struct + generate_caption command

**Files:**
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `run_llm_prompt(&app, &server, &prompt, &model_path, &ollama_model, 1024)`, `extract_json_object(&content)` — keduanya sudah ada di commands.rs
- Produces: struct `CaptionResult`, command `generate_caption(app, server, segments, language, model_path, ollama_model) -> Result<CaptionResult, String>`

- [ ] **Step 1: Tambah `CaptionResult` struct setelah `ClassifyResult`**

Cari blok `pub struct ClassifyResult` (sekitar baris 40-50) dan tambahkan setelahnya:

```rust
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CaptionResult {
    pub caption_short:   String,
    pub caption_long:    String,
    pub hashtags_short:  Vec<String>,
    pub hashtags_long:   Vec<String>,
}
```

- [ ] **Step 2: Tambah `generate_caption` command setelah `classify_transcript`**

Tambahkan setelah penutup `}` dari fungsi `classify_transcript`:

```rust
#[tauri::command]
pub async fn generate_caption(
    app: tauri::AppHandle,
    server: tauri::State<'_, LlamaServerState>,
    segments: Vec<SrtSegment>,
    language: String,
    model_path: String,
    ollama_model: String,
) -> Result<CaptionResult, String> {
    if segments.is_empty() {
        return Err("Tidak ada segmen untuk generate caption".to_string());
    }
    let lang = if language.is_empty() { "id".to_string() } else { language };

    // Build prompt inline — same pattern as build_analyze_prompt
    let text: String = segments.iter()
        .map(|s| format!("[{}] {}: {}", s.index, s.start_time, s.text))
        .collect::<Vec<_>>().join("\n");
    let lang_name = match lang.as_str() {
        "id" => "Bahasa Indonesia",
        "en" => "English",
        _    => "English",
    };
    let prompt = format!(
        "Kamu adalah copywriter media sosial profesional.\n\
         Buat caption untuk video berikut berdasarkan transkripnya.\n\n\
         Bahasa caption: {lang_name}\n\n\
         Transkrip klip:\n{text}\n\n\
         Buat dua versi caption:\n\
         1. TikTok (pendek): hook 1 kalimat kuat, 1-2 kalimat isi, 5-7 hashtag relevan\n\
         2. Instagram (panjang): hook, 2-3 paragraf isi, call-to-action, 15-20 hashtag\n\n\
         Balas HANYA dengan JSON valid tanpa teks lain:\n\
         {{\"caption_short\":\"...\",\"caption_long\":\"...\",\
         \"hashtags_short\":[\"#tag1\"],\"hashtags_long\":[\"#tag1\",\"#tag2\"]}}"
    );

    let content = run_llm_prompt(&app, &*server, &prompt, &model_path, &ollama_model, 1024).await?;
    serde_json::from_str(extract_json_object(&content))
        .map_err(|e| format!("Gagal parse caption dari AI: {e}\nContent: {content}"))
}
```

- [ ] **Step 3: Verifikasi compile**

```bash
cd src-tauri && cargo check 2>&1 | grep "^error" | head -5
```
Expected: tidak ada output (tidak ada error)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(commands): tambah CaptionResult dan generate_caption command"
```

---

### Task 3: `lib.rs` — register generate_caption

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `generate_caption` dari Task 2

- [ ] **Step 1: Tambah `generate_caption` ke invoke_handler**

Tambahkan `generate_caption,` setelah `classify_transcript,` di dalam `tauri::generate_handler![...]`:

```rust
analyze_transcript,
classify_transcript,
generate_caption,
```

- [ ] **Step 2: Verifikasi compile**

```bash
cd src-tauri && cargo check 2>&1 | grep "^error" | head -5
```
Expected: tidak ada output

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(lib): register generate_caption command"
```

---

### Task 4: `types.ts` — tambah CaptionResult interface

**Files:**
- Modify: `src/types.ts`

**Interfaces:**
- Produces: `CaptionResult` interface, dipakai Task 6

- [ ] **Step 1: Tambah interface setelah `ClassifyResult`**

```typescript
export interface CaptionResult {
  caption_short:  string;
  caption_long:   string;
  hashtags_short: string[];
  hashtags_long:  string[];
}
```

- [ ] **Step 2: Commit**

```bash
git add src/types.ts
git commit -m "feat(types): tambah CaptionResult interface"
```

---

### Task 5: `i18n.tsx` — tambah translation keys

**Files:**
- Modify: `src/i18n.tsx`

**Interfaces:**
- Produces: keys `captionTitle`, `captionTabTiktok`, `captionTabInstagram`, `captionGenerating`, `captionBtnCopy`, `captionBtnCopied`, `captionBtnRegenerate`, `captionError`, `captionBtnRetry`

- [ ] **Step 1: Tambah keys ke blok Indonesia (cari `btnBackTranscript` sebagai anchor)**

Tambahkan setelah `btnBackTranscript: "← Kembali ke Transkrip",`:

```typescript
captionTitle: "📋 Caption",
captionTabTiktok: "TikTok",
captionTabInstagram: "Instagram",
captionGenerating: "Membuat caption...",
captionBtnCopy: "Salin",
captionBtnCopied: "Tersalin! ✓",
captionBtnRegenerate: "Buat Ulang",
captionError: "Gagal membuat caption. Pastikan model AI sudah dikonfigurasi.",
captionBtnRetry: "Coba Lagi",
```

- [ ] **Step 2: Tambah keys yang sama ke blok English**

Cari `btnBackTranscript: "← Back to Transcript",` di blok English, tambahkan setelahnya:

```typescript
captionTitle: "📋 Caption",
captionTabTiktok: "TikTok",
captionTabInstagram: "Instagram",
captionGenerating: "Generating caption...",
captionBtnCopy: "Copy",
captionBtnCopied: "Copied! ✓",
captionBtnRegenerate: "Regenerate",
captionError: "Failed to generate caption. Make sure AI model is configured.",
captionBtnRetry: "Try Again",
```

- [ ] **Step 3: Commit**

```bash
git add src/i18n.tsx
git commit -m "feat(i18n): tambah translation keys untuk caption panel"
```

---

### Task 6: `App.tsx` — pass props baru ke ClipResults

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `selectedIndices: Set<number>`, `segments: SrtSegment[]`, `detectedLanguage: string`, `selectedLlm: LlmModel | null` — semuanya sudah ada di state App.tsx
- Produces: props `selectedSegments`, `detectedLanguage`, `modelPath`, `ollamaModel` ke ClipResults

- [ ] **Step 1: Tambah props ke ClipResults di JSX**

Cari blok:
```tsx
<ClipResults
  result={clipResult}
  loading={step === "clipping"}
  onBack={() => setStep("transcript")}
/>
```

Ganti menjadi:
```tsx
<ClipResults
  result={clipResult}
  loading={step === "clipping"}
  onBack={() => setStep("transcript")}
  selectedSegments={segments.filter(s => selectedIndices.has(s.index))}
  detectedLanguage={detectedLanguage}
  modelPath={selectedLlm?.source === "local" ? selectedLlm.path : ""}
  ollamaModel={selectedLlm?.source === "ollama" ? selectedLlm.ollama_model : ""}
/>
```

- [ ] **Step 2: Commit**

```bash
git add src/App.tsx
git commit -m "feat(app): pass selectedSegments dan language ke ClipResults"
```

---

### Task 7: `ClipResults.tsx` — caption panel

**Files:**
- Modify: `src/components/ClipResults.tsx`

**Interfaces:**
- Consumes: `generate_caption` Tauri command (Task 2+3), `CaptionResult` (Task 4), i18n keys (Task 5), props dari Task 6
- Produces: panel caption dengan dua tab + tombol salin

- [ ] **Step 1: Update interface Props dan tambah imports**

Ganti isi file dengan versi baru. Tambah imports di atas:
```typescript
import { invoke } from "@tauri-apps/api/core";
import { useState, useEffect } from "react";
import { useLang } from "../i18n";
import type { ClipResult, CaptionResult, SrtSegment } from "../types";
```

Update interface Props:
```typescript
interface Props {
  result: ClipResult | null;
  loading: boolean;
  onBack: () => void;
  selectedSegments: SrtSegment[];
  detectedLanguage: string;
  modelPath: string;
  ollamaModel: string;
}
```

- [ ] **Step 2: Tambah caption state di dalam komponen**

Tambahkan setelah `const { t } = useLang();`:
```typescript
type CaptionState = "idle" | "loading" | "done" | "error";
const [captionState, setCaptionState] = useState<CaptionState>("idle");
const [caption, setCaption] = useState<CaptionResult | null>(null);
const [captionTab, setCaptionTab] = useState<"tiktok" | "instagram">("tiktok");
const [copied, setCopied] = useState(false);
```

- [ ] **Step 3: Tambah useEffect untuk auto-generate**

Tambahkan setelah deklarasi state:
```typescript
useEffect(() => {
  if (!result || loading || selectedSegments.length === 0) return;
  generateCaption();
}, [result]);

async function generateCaption() {
  setCaptionState("loading");
  setCopied(false);
  try {
    const res = await invoke<CaptionResult>("generate_caption", {
      segments: selectedSegments,
      language: detectedLanguage,
      modelPath,
      ollamaModel,
    });
    setCaption(res);
    setCaptionState("done");
  } catch {
    setCaptionState("error");
  }
}
```

- [ ] **Step 4: Tambah fungsi copy**

Tambahkan setelah `generateCaption`:
```typescript
function copyCaption() {
  if (!caption) return;
  const hashtags = captionTab === "tiktok"
    ? caption.hashtags_short
    : caption.hashtags_long;
  const text = captionTab === "tiktok"
    ? caption.caption_short
    : caption.caption_long;
  navigator.clipboard.writeText(`${text}\n\n${hashtags.join(" ")}`);
  setCopied(true);
  setTimeout(() => setCopied(false), 2000);
}
```

- [ ] **Step 5: Tambah panel caption ke JSX**

Tambahkan sebelum `<div className="results-actions">`:

```tsx
{selectedSegments.length > 0 && (
  <div className="caption-panel">
    <div className="caption-header">
      <span className="caption-title">{t("captionTitle")}</span>
      <button
        className="btn btn-sm btn-secondary"
        onClick={generateCaption}
        disabled={captionState === "loading"}
      >
        {t("captionBtnRegenerate")}
      </button>
    </div>

    {captionState === "loading" && (
      <div className="caption-loading">
        <div className="spinner small" />
        <span>{t("captionGenerating")}</span>
      </div>
    )}

    {captionState === "error" && (
      <div className="caption-error">
        <span>{t("captionError")}</span>
        <button className="btn btn-sm btn-secondary" onClick={generateCaption}>
          {t("captionBtnRetry")}
        </button>
      </div>
    )}

    {captionState === "done" && caption && (
      <>
        <div className="caption-tabs">
          <button
            className={`caption-tab ${captionTab === "tiktok" ? "active" : ""}`}
            onClick={() => setCaptionTab("tiktok")}
          >
            {t("captionTabTiktok")}
          </button>
          <button
            className={`caption-tab ${captionTab === "instagram" ? "active" : ""}`}
            onClick={() => setCaptionTab("instagram")}
          >
            {t("captionTabInstagram")}
          </button>
        </div>

        <div className="caption-body">
          <p className="caption-text">
            {captionTab === "tiktok" ? caption.caption_short : caption.caption_long}
          </p>
          <p className="caption-hashtags">
            {(captionTab === "tiktok" ? caption.hashtags_short : caption.hashtags_long).join(" ")}
          </p>
        </div>

        <div className="caption-footer">
          <button className="btn btn-primary" onClick={copyCaption}>
            {copied ? t("captionBtnCopied") : t("captionBtnCopy")}
          </button>
        </div>
      </>
    )}
  </div>
)}
```

- [ ] **Step 6: Tambah CSS untuk caption panel ke `src/styles.css`**

Tambahkan di akhir file:
```css
.caption-panel {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 16px;
  margin-bottom: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.caption-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.caption-title { font-weight: 600; font-size: 14px; }
.caption-loading, .caption-error {
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--text-secondary);
  font-size: 14px;
}
.caption-tabs {
  display: flex;
  gap: 4px;
  border-bottom: 1px solid var(--border);
  padding-bottom: 8px;
}
.caption-tab {
  background: none;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px 12px;
  font-size: 13px;
  cursor: pointer;
  color: var(--text-secondary);
}
.caption-tab.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.caption-body { display: flex; flex-direction: column; gap: 8px; }
.caption-text {
  font-size: 14px;
  line-height: 1.6;
  white-space: pre-wrap;
  color: var(--text-primary);
}
.caption-hashtags {
  font-size: 13px;
  color: var(--accent);
  word-break: break-word;
}
.caption-footer { display: flex; justify-content: flex-end; }
.btn-sm { padding: 4px 10px; font-size: 12px; }
```

- [ ] **Step 7: Verifikasi TypeScript compile**

```bash
npm run build 2>&1 | grep -E "error TS|Error" | head -10
```
Expected: tidak ada error TypeScript

- [ ] **Step 8: Commit**

```bash
git add src/components/ClipResults.tsx src/styles.css
git commit -m "feat(ui): tambah caption panel di ClipResults — dua tab TikTok/Instagram + salin"
```
