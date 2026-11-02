# model-downloader

Download AI reasoning models on Windows or a Mac, so you can run them offline with llama.cpp, LM Studio or Ollama.

## Download

### Windows

Get `model-downloader.exe` from the [latest release](../../releases/latest). It's a single 3 MB file, and there's nothing to install.

Windows may show "Windows protected your PC" the first time, because the app isn't signed yet. Click **More info**, then **Run anyway**.

### Mac

Open Terminal and run:

```sh
curl -fsSL https://raw.githubusercontent.com/ThuoBrian/model-downloader/main/scripts/install.sh | sh
```

This installs `model-downloader` in `~/.local/bin`. It works on Apple Silicon (M1 and later) and Intel Macs. Run the same command again to update.

If you'd rather download `model-downloader-macos.tar.gz` from the [latest release](../../releases/latest) yourself, macOS will block it the first time because the app isn't signed yet. To allow it, run `xattr -d com.apple.quarantine model-downloader` in the folder you unpacked it to.

## How to use it

1. On Windows, double-click `model-downloader.exe`. On a Mac, type `model-downloader` in Terminal and press Return.
2. Pick a model. The app checks your computer's memory and selects the biggest model that will run well on it.
3. Pick where to save it: an external drive, "This PC" or "This Mac" (your user folder), or any folder you choose.
4. Check the summary and say yes.

Use the arrow keys and Enter. Esc goes back.

Models are saved in a `models` folder, for example `E:\models\qwen3-8b\` or `C:\Users\you\models\qwen3-8b\` on Windows, and `/Volumes/MyDrive/models/qwen3-8b/` or `~/models/qwen3-8b/` on a Mac.

On Apple Silicon Macs, the graphics chip shares the system's memory, so the memory check covers both.

To pause, press Ctrl+C. To resume, pick the same model again. It carries on from where it stopped.

Every download is checked against the checksum Hugging Face or Ollama publishes. If a file arrives damaged, the app deletes it and tells you, so you never end up with a broken model.

## Which model?

The app marks each model for your computer: ✓ runs well, ⚠ tight fit, or ✗ too big. If you're not sure, take the one it selects for you. Bigger models are smarter but need more memory and disk space.

Before a download starts, the app warns you if the model is a tight fit, or if other apps are using so much memory right now that you'll need to close some before running it.

To see what other people are using, choose **Most downloaded models** from the menu. It ranks the models below by Hugging Face downloads over the last month. Some of the most popular ones are too big for many computers, so the app warns you if you pick one that won't run well on yours.

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

## Abliterated models

Abliterated models are versions with their safety refusals removed, so they answer questions the original would decline. That also means they can produce harmful or false content without pushing back, and they may reason slightly worse than the original. They're third-party modifications, and you're responsible for how you use them.

They're kept out of the normal lists. To see them, choose **Abliterated models (uncensored)** from the menu, or run `model-downloader list --abliterated`.

| Model ID | Size |
|---|---|
| qwen3-8b-abliterated | 5.0 GB |
| qwen3-14b-abliterated | 9.0 GB |
| gpt-oss-20b-abliterated | 11.7 GB |

These come from Hugging Face only. The same menu has **Check for more on Hugging Face**, which lists other popular abliterated models. Those aren't checked by this app: many aren't reasoning models, and quality varies.

## Drive format

On Windows, the drive needs to be exFAT or NTFS. On a Mac, use exFAT or APFS. A Mac can read NTFS drives but can't write to them, so the app doesn't list them.

FAT32 can't hold files over 4 GB, and almost every model is bigger than that, so the app won't use a FAT32 drive. To check a drive's format, right-click it in File Explorer and choose Properties, or select it in Finder and press Cmd+I.

## Commands

You can also run it from PowerShell or Terminal:

```sh
model-downloader list                          # all models, rated for your computer
model-downloader list --abliterated            # the uncensored models
model-downloader info qwen3-8b                 # details on one model
model-downloader volumes                       # drives you can save to
model-downloader download qwen3-8b             # download (asks first)
model-downloader download qwen3-32b --quant Q5_K_M
model-downloader download qwen3-8b --source ollama
```

To pick the drive or folder without being asked:

```powershell
# Windows
model-downloader download qwen3-8b --volume E: --yes
model-downloader download qwen3-8b --dir D:\AI
```

```sh
# Mac (--volume takes the drive's name, as shown by `model-downloader volumes`)
model-downloader download qwen3-8b --volume MyDrive --yes
model-downloader download qwen3-8b --dir ~/AI
```

## Release notes

### v0.5.0

The app now runs on Macs as well as Windows.

- Works on Apple Silicon (M1 and later) and Intel Macs. It's one download for both.
- Install it from Terminal with one command (see [Mac](#mac) above). Run the same command again to update.
- The app rates models against your Mac's memory. On Apple Silicon, the graphics chip shares that memory, and the app shows the chip name, for example "Apple M2 Pro (shares system memory)".
- Save to "This Mac" (your home folder), any external drive, or any folder. Pick a drive by name with `--volume MyDrive`.
- Drives a Mac can't write to, such as NTFS drives, aren't listed. The app suggests exFAT or APFS instead.
- You can type `~` in the folder prompt, for example `~/AI`.

Nothing changes on Windows.

## Building it yourself

### Windows

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

### Mac

You need Rust and the Xcode Command Line Tools (`xcode-select --install`).

```sh
cargo build --release
```

The app ends up at `target/release/model-downloader`.

## Publishing a release

Pushing a version tag builds the app on GitHub and attaches `model-downloader.exe` and `model-downloader-macos.tar.gz` (one file for Apple Silicon and Intel Macs) to a new release. The tag has to match the version in `Cargo.toml`:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

For the next release, bump `version` in `Cargo.toml` first, commit, then tag.
