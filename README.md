# pdf-to-md

A fast and efficient PDF to Markdown converter written in Rust using parallel processing. Based on semantic analysis of PDF documents. Quality may vary, but it's cheaper and faster than using OCR. In certain cases, it can be useful for converting documents to Markdown.

## Features

- **High Performance**: Uses all available CPU cores thanks to `rayon` for parallel processing of pages and files. (Note: Performance optimizations are ongoing).
- **Cross-platform**: Automatic download and setup of required PDFium libraries for Windows, Linux, and macOS.
- **Flexibility**: Supports processing of both single files and entire directories.
- **Smart Formatting**: Extracts text while preserving logical structure (headers, paragraphs).

## Requirements

- **Rust** (latest stable version).
- **`tar` utility**: Must be available in the system to extract libraries during the first build (built-in in Windows 10+).

## Installation and Build

The project automatically manages its dependencies (PDFium). You only need to build the project:

```bash
git clone <repository-url>
cd pdf-to-md
cargo build --release
```

During the first `cargo build`, the script will automatically download the correct version of the library (e.g., `pdfium.dll`) and store it in the `lib/` directory in the project root.

## Usage

### Basic Commands

**Convert a single file:**
```bash
cargo run -- input.pdf
```

**Convert all PDFs in a folder:**
```bash
cargo run -- ./my_pdfs/
```

**Specify output directory:**
```bash
cargo run -- input.pdf -o ./output_folder/
```

**Output to console (stdout):**
```bash
cargo run -- input.pdf --stdout
```

### Arguments Reference

| Argument | Description |
| :--- | :--- |
| `INPUT` | Path to a PDF file or a directory containing files. |
| `-o, --output <DIR>` | Directory to save `.md` files (default is current directory). |
| `-n, --name <NAME>` | Output filename (only for single file input). |
| `-s, --stdout` | Print result to console instead of writing to files. |
| `-v, --verbose` | Enable detailed debug information. |

## License

This project is distributed under the MIT License. See the `LICENSE` file for details (if applicable).
