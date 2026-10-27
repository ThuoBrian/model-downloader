# model-downloader

Download AI reasoning models on Windows, so you can run them offline with llama.cpp, LM Studio or Ollama.

## Download

Get `model-downloader.exe` from the [latest release](../../releases/latest). It's a single 3 MB file, and there's nothing to install.

Windows may show "Windows protected your PC" the first time, because the app isn't signed yet. Click **More info**, then **Run anyway**.

## How to use it

1. Double-click `model-downloader.exe`.
2. Pick a model. The app checks your PC's memory and selects the biggest model that will run well on it.
3. Pick where to save it: an external drive, "This PC" (your user folder), or any folder you choose.
4. Check the summary and say yes.

Use the arrow keys and Enter. Esc goes back.

Models are saved in a `models` folder, for example `E:\models\qwen3-8b\` or `C:\Users\you\models\qwen3-8b\`.

To pause, press Ctrl+C. To resume, pick the same model again. It carries on from where it stopped.

Every download is checked against the checksum Hugging Face or Ollama publishes. If a file arrives damaged, the app deletes it and tells you, so you never end up with a broken model.

## Which model?

The app marks each model for your PC: ✓ runs well, ⚠ tight fit, or ✗ too big. If you're not sure, take the one it selects for you. Bigger models are smarter but need more memory and disk space.

Before a download starts, the app warns you if the model is a tight fit, or if other apps are using so much memory right now that you'll need to close some before running it.

To see what other people are using, choose **Most downloaded models** from the menu. It ranks the models below by Hugging Face downloads over the last month. Some of the most popular ones are too big for many PCs, so the app warns you if you pick one that won't run well on yours.

| Model ID | Size |
|---|---|
| dsr1-distill-qwen-7b | 4.7 GB |
| dsr1-distill-llama-8b | 5.2 GB |
| qwen3-8b | 5.2 GB |
| phi-4-reasoning | 9 GB |
| dsr1-distill-qwen-14b | 9.4 GB |
| qwen3-14b | 9.4 GB |
| gpt-oss-20b | 12 GB |
| qwen3-30b-a3b | 18 GB |
| dsr1-distill-qwen-32b | 20 GB |
| qwen3-32b | 20 GB |
| dsr1-distill-llama-70b | 42 GB |
| llama-3.3-70b | 42 GB |
| gpt-oss-120b | 67 GB |

Sizes are for the default quality setting (Q4_K_M). Higher settings like Q8_0 are better but bigger. As a rough guide, a 16 GB laptop runs models up to about 9 GB well.

## Drive format

The drive needs to be exFAT or NTFS. FAT32 can't hold files over 4 GB, and almost every model is bigger than that, so the app won't use a FAT32 drive. To check, right-click the drive in File Explorer and choose Properties.

## Commands

You can also run it from PowerShell:

```powershell
model-downloader list                          # all models, rated for your PC
model-downloader info qwen3-8b                 # details on one model
model-downloader volumes                       # drives you can save to
model-downloader download qwen3-8b             # download (asks first)
model-downloader download qwen3-8b --volume E: --yes
model-downloader download qwen3-8b --dir D:\AI
model-downloader download qwen3-32b --quant Q5_K_M
model-downloader download qwen3-8b --source ollama
```

## Building it yourself

You need Rust and the Visual Studio Build Tools.

```powershell
cargo build --release
```

The app ends up at `target\release\model-downloader.exe`.

To install it with a Desktop shortcut instead, run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Clean
```

This builds the app, copies it to `%LOCALAPPDATA%\Programs\ModelDownloader`, adds a "Model Downloader" shortcut to your Desktop, and deletes the build folder to free up space. Run it again after any changes to update the installed copy.

## Publishing a release

Pushing a version tag builds the app on GitHub and attaches `model-downloader.exe` to a new release. The tag has to match the version in `Cargo.toml`:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

For the next release, bump `version` in `Cargo.toml` first, commit, then tag.
