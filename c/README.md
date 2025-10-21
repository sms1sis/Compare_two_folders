## 📁 Compare Folders in C

A modular C utility to compare two folders using file names and hashes (BLAKE3, SHA256). Outputs colorized results, summary stats, and optional JSON reports.

---

## 🚀 Features

- 🔍 Compare file names and contents
- 🔐 Supports BLAKE3 and SHA256
- 🎨 Colorized terminal output
- 📊 Summary statistics
- 🧾 Optional JSON report

---

## 🧰 Build Instructions

```bash

Install dependencies
sudo apt install libssl-dev

Clone BLAKE3 C library
git clone https://github.com/BLAKE3-team/BLAKE3
cp BLAKE3/c/blake3.* src/

Build the project
make
```

---

## 📦 Usage

```bash
./compare_folders <folder1> <folder2> [--algo=blake3|sha256|both] [--json]
```

Example:

```bash
./compare_folders test/a test/b --algo=sha256 --json
```

---

## 📄 Output

- ✅ Matched files: green
- ❌ Unmatched files: red
- 🧾 JSON report: report.json

---

🧠 Extensibility

Modular design makes it easy to:
- Add new hash algorithms
- Support TXT/CSV output
- Integrate with other tools

---

📜 License

MIT


---
