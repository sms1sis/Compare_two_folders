# Folder File Comparison Utility

A command-line utility for comparing the files in two folders, implemented in both **C** and **Rust**.  
The tool compares files by their names and SHA256 hashes, reporting matches, differences, missing, and extra files.  
The output includes a colorized summary with perfectly aligned results.

---

## ✨ Features

- **Compare two directories**: Checks for files with the same name in both folders.
- **SHA256 hash comparison**: Compares file contents securely using SHA256.
- **Colorized terminal output**: Easy-to-read, informative, and visually appealing output.
- **Summary section**: Lists total files, matches, differences, missing, and extra files, with aligned formatting.
- **Written in both C and Rust**: Choose your preferred language!

---

## ⚙️ Build & Usage

### C Version

#### 📦 Requirements

- GCC or Clang
- OpenSSL development libraries (`libssl-dev` on Debian/Ubuntu)

#### ⚙️ Build

```sh
gcc -o compare_folders compare_folders.c -lssl -lcrypto
```

#### 🚀 Usage

```sh
./compare_folders <folder1> <folder2>
```

#### 📝 Example

```sh
./compare_folders ./dirA ./dirB
```

---

### Rust Version

#### 📦 Requirements

- Rust (https://www.rust-lang.org/tools/install)
- [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)

#### ⚙️  Build

```sh
cargo build --release
```

#### 🚀 Usage

```sh
cargo run -- <folder1> <folder2>
# Or after building:
./target/release/folder_compare <folder1> <folder2>
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

