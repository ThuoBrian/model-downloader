# model-downloader

Download AI reasoning models on Windows or a Mac, so you can run them offline with llama.cpp, LM Studio or Ollama.

## Download

### Windows

Get `model-downloader.exe` from the [latest release](../../releases/latest). It's a single 8 MB file, and there's nothing to install.

Windows may show "Windows protected your PC" the first time, because the app isn't signed yet. Click **More info**, then **Run anyway**.

### Mac

Open Terminal and run:

```sh
curl -fsSL https://raw.githubusercontent.com/ThuoBrian/model-downloader/main/scripts/install.sh | sh
```

This puts **Model Downloader** in the Applications folder in your home folder, so you can open it from Launchpad or Finder. It works on Apple Silicon (M1 and later) and Intel Macs. Run the same command again to update.

If you'd rather download `Model-Downloader-macos.zip` from the [latest release](../../releases/latest) yourself, macOS will block the app the first time because it isn't signed yet. To allow it, run `xattr -dr com.apple.quarantine "Model Downloader.app"` in the folder you unzipped it to.

## How to use it

1. Open the app. On Windows, double-click `model-downloader.exe`. On a Mac, open **Model Downloader** from Launchpad or Applications.
2. Pick a model from the list on the left. The app checks your computer's memory and selects the biggest model that will run well on it.
3. On the right, pick where to save it: an external drive, "This PC" or "This Mac" (your user folder), or **Choose a folder…** for anywhere else.
4. Click **Check & download**, read the summary, and click **Download**.

Models are saved in a `models` folder, for example `E:\models\qwen3-8b\` or `C:\Users\you\models\qwen3-8b\` on Windows, and `/Volumes/MyDrive/models/qwen3-8b/` or `~/models/qwen3-8b/` on a Mac. When a download finishes, **Open folder** takes you there.

On Apple Silicon Macs, the graphics chip shares the system's memory, so the memory check covers both.

To stop partway, click **Pause**. To carry on, click **Resume**, or pick the same model again later, even after closing the app. It picks up from where it stopped. If you close the window during a download, the app asks first and pauses it for you.

Every download is checked against the checksum Hugging Face or Ollama publishes. If a file arrives damaged, the app deletes it and tells you, so you never end up with a broken model.

## Which model?

The app marks each model for your computer: ✔ runs well, ⚠ tight fit, or ✖ too big. If you're not sure, take the one it selects for you. Bigger models are smarter but need more memory and disk space.

Before a download starts, the app warns you if the model is a tight fit, or if other apps are using so much memory right now that you'll need to close some before running it.

To see what other people are using, open the **Most downloaded** tab. It ranks the models below by Hugging Face downloads over the last month. Some of the most popular ones are too big for many computers, and the list marks those.

| Model | Size |
|---|---|
| DeepSeek-R1-Distill-Qwen-7B | 4.7 GB |
| DeepSeek-R1-Distill-Llama-8B | 5.2 GB |
| Qwen3-8B | 5.2 GB |
| Phi-4-Reasoning | 9 GB |
| DeepSeek-R1-Distill-Qwen-14B | 9.4 GB |
| Qwen3-14B | 9.4 GB |
| GPT-OSS-20B | 12 GB |
| Qwen3-30B-A3B (MoE) | 18 GB |
| DeepSeek-R1-Distill-Qwen-32B | 20 GB |
| Qwen3-32B | 20 GB |
| DeepSeek-R1-Distill-Llama-70B | 42 GB |
| Llama-3.3-70B-Instruct | 42 GB |
| GPT-OSS-120B | 67 GB |

Sizes are for the default quality setting (Q4_K_M). Higher settings like Q8_0 are better but bigger. As a rough guide, a 16 GB laptop runs models up to about 9 GB well.

## Abliterated models

Abliterated models are versions with their safety refusals removed, so they answer questions the original would decline. That also means they can produce harmful or false content without pushing back, and they may reason slightly worse than the original. They're third-party modifications, and you're responsible for how you use them.

They're kept out of the normal list. To see them, open the **Abliterated** tab.

| Model | Size |
|---|---|
| Qwen3-8B (abliterated) | 5.0 GB |
| Qwen3-14B (abliterated) | 9.0 GB |
| GPT-OSS-20B (abliterated) | 11.7 GB |

These come from Hugging Face only. The same tab has **Check Hugging Face for more…**, which lists other popular abliterated models. Those aren't checked by this app: many aren't reasoning models, and quality varies.

## Drive format

On Windows, the drive needs to be exFAT or NTFS. On a Mac, use exFAT or APFS. A Mac can read NTFS drives but can't write to them, so the app doesn't list them.

FAT32 can't hold files over 4 GB, and almost every model is bigger than that, so the app greys out FAT32 drives. To check a drive's format, right-click it in File Explorer and choose Properties, or select it in Finder and press Cmd+I.

## Release notes

### v0.6.0

The app now opens in a window instead of a terminal.

- Pick a model, a quality setting and where to save it, all on one screen. Click **Choose a folder…** to save anywhere.
- Downloads show a progress bar with speed and time left. **Pause** and **Resume** replace Ctrl+C.
- If you close the window during a download, the app asks first, then pauses it so you can carry on later.
- When a download finishes, **Open folder** shows the file, and **Copy llama.cpp command** copies the command to run it.
- On a Mac, the app is now a proper app in your Applications folder. The install command removes the old command-line version.
- The command line (`model-downloader list`, `download` and so on) is gone. Everything it did is in the window.

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

You need Rust 1.95 or later.

### Windows

You also need the Visual Studio Build Tools.

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

You also need the Xcode Command Line Tools (`xcode-select --install`).

```sh
cargo build --release
```

The app ends up at `target/release/model-downloader`. Running it opens the window. The release workflow wraps it in `Model Downloader.app`.

## Publishing a release

Pushing a version tag builds the app on GitHub and attaches `model-downloader.exe` and `Model-Downloader-macos.zip` (one app for Apple Silicon and Intel Macs) to a new release. The tag has to match the version in `Cargo.toml`:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

For the next release, bump `version` in `Cargo.toml` first, commit, then tag.
