## Folder File Comparison Utility (cmpf)

A command-line utility for comparing the files in two folders, implemented in **Rust**.   
The tool compares files by their names and hashes, reporting matches, differences, missing, and extra files. It offers flexible comparison modes and hashing algorithms.

---

## ✨ Features

- **Compare two directories**: Checks for files with the same name in both folders.
- **Flexible Hashing**: Compares file contents securely using `Sha256`, `Blake3`, or `Both` algorithms (default blake3).
- **Comparison Modes**:
    - `Batch` (default): Processes files in parallel for maximum speed, generating a comprehensive report at the end.
    - `Realtime`: Processes files sequentially, providing immediate output as each file is compared.
- **Progress Bar**: Shows a progress bar in `Batch` mode.
- **Colorized Terminal Output**: Easy-to-read, informative, and visually appealing output for both real-time feedback and final reports.
- **Enhanced Summary Section**: A clear, colorized, and perfectly aligned summary box detailing total files, matches, differences, missing, extra files, mode, algorithm used, and time taken.
- **JSON and TXT Output**: Option to save the comparison report as a `json` or `txt` file.

---

## ⚙️ Build & Usage

#### 📦 Requirements

- Rust (https://www.rust-lang.org/tools/install)
- [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)

#### ⚙️  Build

```sh
cargo build --release
```

#### 🚀 Usage

```sh
# from rust/ folder
cargo build --release

./target/release/cmpf ./dirA ./dirB

# Run in Realtime mode (sequential output)
./target/release/cmpf ./dirA ./dirB -m realtime

# Run in Batch mode (parallel processing, report at end)
./target/release/cmpf ./dirA ./dirB -m batch

# Run with only BLAKE3 algorithm (default)
./target/release/cmpf ./dirA ./dirB -a blake3

# Run with only SHA-256 algorithm
./target/release/cmpf ./dirA ./dirB -a sha256

# Run with both algorithms
./target/release/cmpf ./dirA ./dirB -a both

# Save report as a text file (Batch mode only)
./target/release/cmpf ./dirA ./dirB -o=./reports -f=txt

# Save report as a JSON file (Batch mode only)
./target/release/cmpf ./dirA ./dirB -o=./reports -f=json
```

#### 📝 Example

```sh
cargo run -- test_folder1 test_folder2 -m realtime
```

---

## 🖥️ Example Output (Realtime Mode)

```
===============================================
   Folder Comparison Utility (cmpf - Real-time Mode)
===============================================
[MATCH]  common.txt
    in_both: sha256:cd575532bfb6aa856c11dcdc1c68c99a0bf0fc5b42d575392ac07c950e9f426f blake3:5c8c5b280826a57d2f55c48aaa2fbf0b1703ddb44831958578549897da3563a3

[DIFF]  different.txt
    folder1: sha256:62e4c9fd0489743c376e09101368dd0e38b25f8a9b49d3d34c2c9942cb3d8b04 blake3:4034107a698e6c3a4576cabc9e3231fe0c79b45537674590a42443882837cc60
    folder2: sha256:d33681ea2887e73666e1dfa572ad932217bd0fa20781f48979bcfc07b8fbb22b blake3:ca856568c194199bad3e86801b7d7608f8e44510c66ac69e472def34bc7c6dfc

[MISSING]  unique1.txt

[EXTRA]  unique2.txt

╔══════════════════════════════════════════════╗
║                    Summary                           ║
╠══════════════════════════════════════════════╣
║  Mode                   : Realtime                   ║
║  Algorithm              : Both                       ║
║  Total files checked    : 4                          ║
║  Matches                : 1                          ║
║  Differences            : 1                          ║
║  Missing in Folder2     : 1                          ║
║  Extra in Folder2       : 1                          ║
║  Time taken             : 5.23ms                     ║
╚══════════════════════════════════════════════╝
```

## ⚡Note:
- For fastest speed use default setup. **blake3+batch**
- If you don't pass any flag it will fall back to default 

---

## 🙌 Credits

- [`anyhow`](https://crates.io/crates/anyhow) - [`blake3`](https://crates.io/crates/blake3) - [`clap`](https://crates.io/crates/clap) - [`colored`](https://crates.io/crates/colored) - [`rayon`](https://crates.io/crates/rayon) - [`serde`](https://crates.io/crates/serde) - [`serde_json`](https://crates.io/crates/serde_json) - [`sha2`](https://crates.io/crates/sha2) - [`walkdir`](https://crates.io/crates/walkdir)

---

## Contributions

Pull requests and improvements are welcome! Please open an issue first if you wish to discuss major changes.
