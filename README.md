# Folder File Comparison Utility

A command-line utility for comparing the files in two folders, implemented in **Rust**.   
The tool compares files by their names and **blake3** **sha256**  hashes, reporting matches, differences, missing, and extra files.  

---

## ✨ Features

- **Compare two directories**: Checks for files with the same name in both folders.
- **blake3 and  sha256 hash comparison**: Compares file contents securely using blake3, sha256 or both.
- **Colorized terminal output**: Easy-to-read, informative, and visually appealing output.
- **Summary section**: Lists total files, matches, differences, missing, and extra files, with aligned formatting.
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
# run with both hashes (default)
cargo run -- ./dirA ./dirB
# run with only BLAKE3
cargo run -- ./dirA ./dirB --algo=blake3
# run with only SHA-256
cargo run -- ./dirA ./dirB --algo=sha256
```

#### 📝 Example

```sh
cargo run -- ./dirA ./dirB
```

---

## 🖥️ Example Output

```
===============================================
   Folder File Comparison Utility by sms1sis         
===============================================

         Comparing files in folders:            
           Folder 1: ./dirA                     
           Folder 2: ./dirB                     
-----------------------------------------------

[MATCH]   file1.txt
[DIFF]    file2.txt
[MISSING] file3.txt not found in Folder2
[EXTRA]   file4.txt only in Folder2

-----------------------------------------------
                 Summary
-----------------------------------------------
Total files checked  : 3
Matches              : 1
Differences          : 1
Missing in Folder2   : 1
Extra in Folder2     : 1
===============================================
```

---

## 🙌 Credit

- [OpenSSL](https://www.openssl.org/) for C SHA256 implementation
- [`sha2`](https://crates.io/crates/sha2), [`colored`](https://crates.io/crates/colored), and [`terminal_size`](https://crates.io/crates/terminal_size) Rust crates

---

## Contributions

Pull requests and improvements are welcome! Please open an issue first if you wish to discuss major changes.

