# Reverse Engineering Binary Formats Workflow

Understanding undocumented file formats, network protocols, or binary data structures through systematic analysis.

## Trigger

- Unknown file format needs parsing
- Protocol needs implementation without spec
- Legacy system produces opaque data
- Forensic analysis of data files

## Goal

- Documented format specification
- Working parser/decoder
- Test corpus with known interpretations
- Confidence level for each field

## Prerequisites

- Sample files (more = better)
- Ability to generate/modify test files (if possible)
- Hex editor or binary analysis tools
- Optional: tool that produces the format (for experimentation)

## Why This Is Hard

Binary reverse engineering challenges:
1. **No semantic markers**: Unlike text, no keywords to search for
2. **Multiple valid interpretations**: Is `0x0000001A` a length, offset, enum, or flags?
3. **Context-dependent meaning**: Same bytes mean different things in different positions
4. **Endianness ambiguity**: Little-endian? Big-endian? Mixed?
5. **Compression/encryption**: Data may be transformed

## Core Strategy: Hypothesis-Driven Differential Analysis

```
┌─────────────────────────────────────────────────────────┐
│                    Sample Collection                     │
│  Gather diverse samples: minimal, maximal, edge cases   │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                   Pattern Recognition                    │
│  Find invariants: magic bytes, fixed structures         │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                  Hypothesis Formation                    │
│  "Bytes 4-7 are a uint32 length field"                  │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     Verification                         │
│  Test hypothesis across all samples                      │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                 Refinement / Iteration                   │
│  Adjust hypothesis, document confidence                  │
└─────────────────────────────────────────────────────────┘
```

## Decomposition Strategy

### Phase 1: Sample Collection & Triage

```
1. GATHER samples
   - Minimum viable file (smallest valid)
   - Maximum complexity file (all features used)
   - Edge cases (empty collections, max values)
   - Invalid files (if available - shows validation)

2. CATEGORIZE samples
   - Group by size, apparent complexity
   - Note creation context (what tool/version)
   - Identify "Rosetta stone" samples (known content)

3. COMPUTE basic statistics
   - File sizes (look for patterns: all multiples of N?)
   - Entropy analysis (high entropy = compressed/encrypted)
   - Byte frequency distribution
```

### Phase 2: Structure Discovery

```
4. FIND invariants (magic bytes, headers)
   - Compare file starts (common prefix?)
   - Compare file ends (common suffix? checksum?)
   - Look for repeating patterns

5. IDENTIFY structure boundaries
   - Null byte clusters (padding/alignment)
   - Length-prefixed sections
   - Sentinel values / delimiters

6. MAP coarse structure
   - Header region
   - Data region(s)
   - Index/directory region
   - Footer region
```

### Phase 3: Field-Level Analysis

```
7. HYPOTHESIZE field meanings
   For each suspected field:
   - What type? (int, float, string, offset, enum)
   - What endianness?
   - What does it represent?

8. DIFFERENTIAL analysis
   - Change one thing in source → observe what changes in binary
   - Correlate field values with known properties

9. CROSS-VALIDATE across samples
   - Does hypothesis hold for ALL samples?
   - Document confidence level per field
```

### Phase 4: Parser Implementation

```
10. WRITE incremental parser
    - Start with header
    - Add fields as confirmed
    - Fail loudly on unexpected values

11. ROUND-TRIP test (if possible)
    - Parse → modify → serialize → parse again
    - Confirms understanding is complete

12. DOCUMENT specification
    - Field-by-field documentation
    - Diagrams for complex structures
    - Confidence levels noted
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Collect | File system, sample generators |
| Triage | `xxd`, `file`, entropy tools |
| Structure | Hex editor, `binwalk`, custom scripts |
| Analysis | `view` (hex), diff tools, LLM reasoning |
| Parser | Code editor, test framework |
| Document | Markdown, diagrams |

### For Human-Driven Analysis: ImHex

[ImHex](https://github.com/WerWolv/ImHex) is a hex editor purpose-built for reverse engineering. GUI-based, so requires human interaction, but excellent for the workflow:

- **Pattern language**: Define struct layouts declaratively, auto-highlights matching bytes
- **Data inspector**: View selection as various types simultaneously
- **Entropy visualization**: Built-in entropy graph overlay
- **Diff view**: Compare two files side-by-side

```cpp
// ImHex pattern example
struct Header {
    char magic[4];     // Expect "SAVE"
    u32 version;
    u32 file_size;
    u32 checksum;
};
```

Similar GUI tools: 010 Editor (commercial), Hex Fiend (macOS)

### For LLM-Driven Analysis: CLI Tools

LLMs need text-based tools with parseable output:

```bash
# Basic inspection
xxd file.dat | head -50          # Hex dump
xxd -s 0x100 -l 64 file.dat      # Specific region
file file.dat                     # Format detection
strings -n 8 file.dat            # Extract strings

# Structure analysis
binwalk file.dat                 # Signature scan
binwalk -E file.dat              # Entropy graph (text)
binwalk --raw=-e file.dat        # Extract embedded files

# Comparison
diff <(xxd a.dat) <(xxd b.dat)   # Hex diff
cmp -l a.dat b.dat               # Byte-by-byte diff

# Compute values
od -A x -t x4 -N 16 file.dat     # Read as 32-bit ints
printf '%d\n' 0x$(xxd -s 4 -l 4 -p file.dat | tac -rs ..)  # LE int at offset 4
```

For programmatic analysis, Python scripts work well:
```python
import struct

with open('file.dat', 'rb') as f:
    magic = f.read(4)
    version = struct.unpack('<I', f.read(4))[0]  # Little-endian uint32
    print(f"Magic: {magic}, Version: {version}")
```

**Pattern languages** bridge GUI and CLI - LLM writes the format description, runtime validates against bytes:

[ImHex Pattern Language](https://github.com/WerWolv/PatternLanguage) - standalone runtime, no GUI needed:
```cpp
// format.hexpat - LLM can write this
struct Header {
    char magic[4];
    u32 version;
    u32 file_size;
    u32 checksum;
};

Header header @ 0x00;  // Place at offset 0
```
Rich type system, pattern matching, conditionals, bitfields. Same language as ImHex GUI but usable from CLI.

**Kaitai Struct** - declarative YAML, generates parsers in multiple languages:
```yaml
# format.ksy
meta:
  id: save_file
  endian: le
seq:
  - id: magic
    contents: "SAVE"
  - id: version
    type: u4
  - id: file_size
    type: u4
```
Then: `kaitai-struct-compiler format.ksy --target python` → usable parser

Both are LLM-friendly: write text description → validate against binary → iterate.

Other CLI tools:
- **radare2/rizin**: `r2 -q -c 'px 64' file.dat` for scripted analysis
- **yara**: Pattern matching rules (text-based, LLM-friendly)
- **foremost/scalpel**: File carving from binary blobs

## Core Techniques

### Finding Struct Sizes

**Problem**: You suspect there's an array of structs, but don't know the struct size.

**Technique 1: Item count correlation**
```
If you know the item count (from UI, filename, or header field):
  struct_size = data_region_size / item_count

Example:
  - File contains 10 inventory items (known from game UI)
  - Data region is 0x100-0x1F0 (240 bytes)
  - Struct size = 240 / 10 = 24 bytes
```

**Technique 2: Differential - add/remove items**
```
Create two files:
  - File A: 5 items
  - File B: 6 items
  - Diff size = struct_size (+ possible index overhead)

Example:
  save_5items.dat: 1,024 bytes
  save_6items.dat: 1,048 bytes
  Difference: 24 bytes → struct is likely 24 bytes
```

**Technique 3: Repeating pattern detection**
```
Look for repeating byte patterns at fixed intervals:

Offset 0x100: 01 00 00 00 FF FF 00 00 ...
Offset 0x118: 02 00 00 00 FF FF 00 00 ...  (0x18 = 24 bytes later)
Offset 0x130: 03 00 00 00 FF FF 00 00 ...  (0x18 = 24 bytes later)

Pattern repeats every 24 bytes → struct size = 24
First field looks like an incrementing ID
```

**Technique 4: Alignment analysis**
```
Most formats align structs to 4 or 8 byte boundaries.
If data region is 240 bytes with unknown count:
  240 / 4 = 60 (possible: 60 × 4-byte, 30 × 8-byte, 20 × 12-byte, etc.)
  240 / 8 = 30
  240 / 24 = 10

Cross-reference with other evidence (item counts, patterns)
```

### Finding Length-Prefixed Values

**Problem**: Variable-length data (strings, arrays, blobs) - where's the length?

**Technique 1: Known string search**
```
If you know a string exists in the file (e.g., player name "Alice"):

1. Find "Alice" in hex dump
   Offset 0x200: 41 6C 69 63 65 00  ("Alice" + null)

2. Look at bytes immediately before
   Offset 0x1FE: 05 00              (little-endian 5 = length of "Alice")
   Offset 0x200: 41 6C 69 63 65 00

3. Verify with different length string
   Player name "Bob" (3 chars)
   Offset 0x1FE: 03 00
   Offset 0x200: 42 6F 62 00
```

**Technique 2: Length field candidates**
```
Common length prefix patterns:

1-byte length (max 255):
  05 48 65 6C 6C 6F  →  length=5, "Hello"

2-byte little-endian (max 65535):
  05 00 48 65 6C 6C 6F  →  length=5, "Hello"

4-byte little-endian:
  05 00 00 00 48 65 6C 6C 6F  →  length=5, "Hello"

Varint (protobuf-style):
  05 48 65 6C 6C 6F  →  length=5, "Hello"
  AC 02 ...         →  length=300 (0x80 bit = continuation)
```

**Technique 3: Differential - vary the length**
```
Create files with different length values:

File A: name = "A"      (1 char)
File B: name = "AAAAA"  (5 chars)

Diff shows:
  - Length field location (value changed from 1 to 5)
  - String location (different content)
  - Whether null-terminated (extra 00 byte?)
```

**Technique 4: Scan for plausible lengths**
```python
# Scan for bytes that could be lengths
def find_length_candidates(data, offset):
    candidates = []

    # Try 1-byte length
    len1 = data[offset]
    if 0 < len1 < 256 and offset + 1 + len1 <= len(data):
        following = data[offset+1 : offset+1+len1]
        if looks_like_string(following):
            candidates.append(('1-byte', len1, offset+1))

    # Try 2-byte little-endian
    len2 = data[offset] | (data[offset+1] << 8)
    if 0 < len2 < 10000 and offset + 2 + len2 <= len(data):
        following = data[offset+2 : offset+2+len2]
        if looks_like_string(following):
            candidates.append(('2-byte-le', len2, offset+2))

    return candidates
```

### Finding Offsets and Pointers

**Problem**: Data references other data by position.

**Technique 1: Self-referential scan**
```
Look for values that match valid offsets within the file:

File size: 0x1000 (4096 bytes)
Scan for 4-byte values in range [0, 0x1000]:

Offset 0x10: 00 02 00 00  →  0x200 (valid offset!)
             Check what's at 0x200 - does it look like start of data?

Offset 0x14: 00 08 00 00  →  0x800 (valid offset!)
             Might be a table of pointers
```

**Technique 2: Differential - move data around**
```
If you can control data placement:

File A: Put "marker" string at natural position
File B: Add padding before "marker" to shift it

If offsets are absolute:
  - Pointer values change by the padding amount

If offsets are relative:
  - Pointer values stay the same
```

### Finding Enums and Flags

**Problem**: Small integers with semantic meaning.

**Technique 1: Enumerate all values**
```
Collect all samples, extract the byte at suspected enum offset:

Sample 1: 0x00
Sample 2: 0x01
Sample 3: 0x00
Sample 4: 0x02
Sample 5: 0x01

Values seen: {0, 1, 2}
Probably an enum with 3 values, not a counter or size
```

**Technique 2: Correlate with behavior**
```
If byte at 0x50 correlates with item type:
  Sword items  → 0x01
  Shield items → 0x02
  Potion items → 0x03

Confirms it's an item_type enum
```

**Technique 3: Bitfield detection**
```
If values are powers of 2 or combinations:
  0x01, 0x02, 0x04, 0x03, 0x05, 0x07

Likely a bitfield:
  bit 0 (0x01): flag A
  bit 1 (0x02): flag B
  bit 2 (0x04): flag C

0x07 = all flags set
```

### Dealing with Unknown Regions

**Problem**: Chunk of bytes with no obvious meaning.

**Technique 1: Entropy analysis**
```
Calculate entropy of region:
  - Low entropy (< 4): structured data, many zeros/patterns
  - Medium entropy (4-6): text, code, mixed data
  - High entropy (> 7): compressed or encrypted
```

**Technique 2: Isolation**
```
Create minimal file with just header + unknown region:
  - Does it still load?
  - Can you zero out the region?
  - Can you truncate it?

Determines if region is required and validated
```

**Technique 3: Mutation testing**
```
Flip random bits in unknown region:
  - File still works → probably padding or reserved
  - File breaks immediately → checksum or critical data
  - File works but behavior changes → functional data
```

## Blind Analysis (No Prior Knowledge)

When you have only the bytes - no known content, no way to generate samples, no verification.

### Structural Heuristics

**Magic bytes / signatures**
```
Always run `file` first - it handles thousands of formats:
  $ file unknown.dat
  → "PNG image data, 800 x 600, 8-bit/color RGB"
  → "ELF 64-bit LSB executable, x86-64"
  → "data"  (unknown - manual analysis needed)

When `file` says "data" or you need more detail:

First 2-16 bytes often identify format:
  89 50 4E 47 0D 0A 1A 0A  → PNG
  50 4B 03 04              → ZIP/DOCX/JAR/APK (all ZIP-based)
  50 4B 05 06              → Empty ZIP
  7F 45 4C 46              → ELF
  4D 5A                    → DOS/Windows executable (MZ)
  CA FE BA BE              → Java class / Mach-O fat binary
  25 50 44 46              → PDF (%PDF)
  52 49 46 46 xx xx xx xx 57 41 56 45 → WAV (RIFF....WAVE)
  1F 8B                    → gzip
  FD 37 7A 58 5A 00        → xz
  42 5A 68                 → bzip2 (BZh)

Signature databases for manual lookup:
  - Gary Kessler's file signatures (gck.net)
  - TrID file identifier
  - Wikipedia "List of file signatures"

Even unknown/proprietary formats often start with ASCII identifier:
  "SAVE", "DATA", "RIFF", company name, format version string
```

**Common header patterns**
```
Many formats follow: MAGIC + VERSION + SIZE + FLAGS + DATA

Offset 0x00: Magic (4-8 bytes, often readable ASCII)
Offset 0x04: Version (usually small int: 1, 2, 0x0100)
Offset 0x08: Total size or section count
Offset 0x0C: Flags or feature bits
Offset 0x10: Data begins or offset to data

Check if bytes 4-7 look like a version (01 00 00 00 = v1)
Check if bytes 8-11 equal file size (self-describing)
```

**TLV (Type-Length-Value) detection**
```
Very common pattern for extensible formats:

  [type: 1-4 bytes] [length: 1-4 bytes] [value: length bytes]

Signs of TLV:
  - Repeating small integers followed by varying data
  - Length values that sum to region size
  - Type values from small set (0-20 typically)

Scan for: if bytes[i+4:i+8] interpreted as length L,
          does bytes[i+8:i+8+L] end at another plausible TLV?
```

### Statistical Analysis

**Entropy mapping**
```
Slide a window across file, compute entropy for each region:

entropy(bytes) = -Σ p(b) * log2(p(b)) for each byte value b

Plot entropy vs offset:
  ┌────────────────────────────────────────┐
  │▁▁▁▁▂▂▃▃▇▇▇▇▇▇▇▇▇▇▃▃▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁│
  └────────────────────────────────────────┘
   ^^^^                ^^^^^^^^^^^^^^^^^^^^
   header (structured) compressed data      padding?

Transitions indicate structure boundaries
```

**Byte frequency distribution**
```
Different data types have characteristic distributions:

Text (ASCII):      Spike at 0x20 (space), 0x61-0x7A (lowercase)
Code (x86):        Spikes at common opcodes (0x89, 0x8B, 0xE8)
Compressed:        Nearly uniform distribution
Encrypted:         Perfectly uniform distribution
Nulls/padding:     Spike at 0x00
Pointers (32-bit): Spikes at 0x00 (high bytes of small addresses)

Histogram reveals data type without knowing content
```

**Autocorrelation (period detection)**
```
For each offset d, compute correlation of data[i] with data[i+d]

Peaks indicate repeating structures:
  - Peak at d=4: might be array of uint32
  - Peak at d=24: might be array of 24-byte structs
  - Peak at d=512: might be block-aligned format

autocorr(d) = Σ (data[i] == data[i+d]) for all valid i
```

**Byte pair analysis (bigram frequency)**
```
Consecutive byte pairs reveal data type more distinctively than single bytes:

Data type signatures by bigram distribution:
  Text (ASCII):    Spikes at "th", "he", "in", "er", "e ", " t"
  Executable:      Instruction patterns (x86: 89 xx, 8B xx, E8 xx)
  Compressed:      Relatively uniform, fewer spikes
  Encrypted:       Nearly perfectly uniform

Higher-level classification (per research literature):
  - Temporal data (audio, video frames): high autocorrelation between
    adjacent samples, periodic patterns from frame boundaries
  - Spatial data (images): 2D correlation patterns, pixel adjacency
  - Spatiotemporal (video): both frame-level periodicity and spatial
    redundancy within frames
  - Non-spatiotemporal (code, archives): structural patterns but no
    inherent dimensional correlation

Bigram entropy calculation:
  bigram_freq[pair] = count(pair) / (len(data) - 1)
  bigram_entropy = -Σ p * log2(p) for each pair frequency p

  Low entropy (< 10):   Highly structured/repetitive
  Medium entropy (10-14): Mixed data, markup, structured binary
  High entropy (> 14):   Compressed or encrypted

This separates file types more effectively than single-byte analysis
because it captures local structure.
```

### Data Type Signatures

**Floating point detection**
```
IEEE 754 floats have recognizable patterns:

32-bit float "1.0":     00 00 80 3F (little-endian)
32-bit float "-1.0":    00 00 80 BF
64-bit double "1.0":    00 00 00 00 00 00 F0 3F

Common floats (0.0, 1.0, -1.0, 0.5, 100.0) are landmarks
Scan for these exact byte patterns

Also: exponent byte (0x3F, 0x40, 0x3E for floats near 1.0)
      appears frequently in float arrays
```

**Timestamp detection**
```
Unix timestamps (seconds since 1970):
  Current era: 0x5xxxxxxx to 0x6xxxxxxx (2004-2033)

  67 89 AB 01 (little-endian) = 0x01AB8967 = 2022-ish

Windows FILETIME (100ns since 1601):
  Much larger numbers, often in pairs

Scan for plausible timestamp values, check if they decode
to reasonable dates
```

**String and encoding detection**
```
The `file` command handles most cases - use it first:
  $ file unknown.dat
  → "UTF-8 Unicode text" / "ISO-8859 text" / "data"

When `file` is unavailable or ambiguous, detect manually:

BOM (Byte Order Mark) signatures - very reliable:
  EF BB BF        → UTF-8 BOM
  FF FE           → UTF-16 LE
  FE FF           → UTF-16 BE
  FF FE 00 00     → UTF-32 LE
  00 00 FE FF     → UTF-32 BE

UTF-8 validation (no BOM):
  0xxxxxxx                          → ASCII (0x00-0x7F)
  110xxxxx 10xxxxxx                 → 2-byte (0xC0-0xDF, 0x80-0xBF)
  1110xxxx 10xxxxxx 10xxxxxx        → 3-byte (0xE0-0xEF, ...)
  11110xxx 10xxxxxx 10xxxxxx 10xx   → 4-byte (0xF0-0xF7, ...)

  If all sequences valid → likely UTF-8
  Invalid sequences (0xC0-0xC1, 0xF5-0xFF, wrong continuations) → not UTF-8

UTF-16 detection (no BOM):
  ASCII text appears as: 48 00 65 00 6C 00 6C 00 6F 00 ("Hello" LE)
                     or: 00 48 00 65 00 6C 00 6C 00 6F ("Hello" BE)
  Look for alternating 0x00 with ASCII range bytes

Shift-JIS (Japanese):
  ASCII range (0x20-0x7F) passes through
  Half-width katakana: 0xA1-0xDF (single byte)
  Two-byte: first byte 0x81-0x9F or 0xE0-0xFC
  Distinctive: frequent 0x82, 0x83 (hiragana/katakana lead bytes)

Other encodings:
  ISO-8859-1: 0x80-0xFF used for accented chars (no multi-byte)
  GB2312/GBK: lead bytes 0xA1-0xF7, similar to Shift-JIS structure
  EUC-JP: 0x8E for half-width kana, 0xA1-0xFE for JIS X 0208

General heuristic:
  - >90% in 0x20-0x7E → ASCII
  - Alternating 0x00 → UTF-16
  - Valid UTF-8 multi-byte sequences → UTF-8
  - 0x80-0xFF with specific lead byte patterns → CJK encoding
```

### Endianness Detection

**Technique 1: High/low byte grouping**
```
Multi-byte integers usually have predictable high byte patterns:
  - Small values (< 256): high bytes are 0x00
  - File offsets: high bytes near 0x00 (files rarely > 16MB)
  - Counters/IDs: increment in low byte first

Scan for grouped patterns (32-bit example):

Little-endian small values:  XX 00 00 00  (common)
Big-endian small values:     00 00 00 XX  (common)
Little-endian medium values: XX XX 00 00
Big-endian medium values:    00 00 XX XX

Heuristic algorithm:
  le_score = count of (data[i+1] == 0x00 AND data[i] != 0x00) at 4-byte boundaries
  be_score = count of (data[i] == 0x00 AND data[i+3] != 0x00) at 4-byte boundaries

  if le_score >> be_score → little-endian
  if be_score >> le_score → big-endian

Also works for 16-bit: look for XX 00 vs 00 XX patterns at 2-byte boundaries

Example:
  Data: 05 00 00 00 | 0A 00 00 00 | 64 00 00 00
        ^^           ^^           ^^
  Low bytes have values, high bytes are zero → little-endian integers 5, 10, 100
```

**Technique 2: Offset verification**
```
If a candidate value might be a pointer/offset:

1. Interpret as little-endian → target_le
2. Interpret as big-endian → target_be
3. Check which target is valid and meaningful

Example:
  Bytes at 0x10: 00 02 00 00

  As LE (read backward): 0x00000200 = 512
  As BE (read forward):  0x00020000 = 131072

  File size: 1024 bytes
  → LE (512) points inside file, BE (131072) points outside
  → Strong evidence for little-endian

Verification chain:
  For each suspected offset field:
    a. Compute both interpretations
    b. Check bounds: target < file_size?
    c. Check alignment: target % 4 == 0? (common for structs)
    d. Check content: does file[target] look like data start?
       - Non-zero bytes (not padding)
       - Reasonable values (not 0xFFFFFFFF)
       - Maybe recognizable pattern (string, float, magic)

Score each interpretation across all suspected offset fields.
Endianness with more valid-looking pointers wins.
```

**Technique 3: Cross-reference with known structures**
```
If you've identified any field with known semantics:

File size field (if file is self-describing):
  - Actual file size: 4096 = 0x1000
  - Bytes: 00 10 00 00 → LE = 0x00001000 ✓
  - Bytes: 00 00 10 00 → BE = 0x00001000 ✓ (ambiguous!)

But with file size 260 = 0x104:
  - Bytes: 04 01 00 00 → LE = 0x104 ✓
  - Bytes: 04 01 00 00 → BE = 0x04010000 ✗

Array counts:
  - If header says "5 items" and you see: 05 00 00 00 → LE
  - If header says "5 items" and you see: 00 00 00 05 → BE

Version numbers:
  - Version 1.0 encoded as two 16-bit: 01 00 00 00 → LE (1, 0)
  - Version 1.0 encoded as two 16-bit: 00 01 00 00 → BE (1, 0)
```

**Mixed endianness warning**
```
Some formats use different endianness for different sections:
  - Network protocols: big-endian headers, native-endian payload
  - File formats: little-endian metadata, big-endian media data

Signs of mixed endianness:
  - One interpretation works for header, fails for body
  - Values in different regions require different swaps

Handle by analyzing regions independently.
```

### Pointer Analysis (Blind)

**Self-reference detection**
```python
def find_internal_pointers(data):
    """Find values that could be offsets within the file"""
    file_size = len(data)
    candidates = []

    for offset in range(0, len(data) - 4, 4):
        # Try as 32-bit little-endian
        value = int.from_bytes(data[offset:offset+4], 'little')

        if 0 < value < file_size:
            # Points within file - suspicious!
            # Extra confidence if target looks like start of structure
            target = data[value:value+4] if value + 4 <= file_size else b''
            candidates.append((offset, value, target))

    # Cluster analysis: multiple pointers to same region = table
    return candidates
```

**Offset table detection**
```
Consecutive pointers often appear in tables:

Offset 0x100: 00 02 00 00  → 0x200
Offset 0x104: 40 02 00 00  → 0x240
Offset 0x108: 80 02 00 00  → 0x280
Offset 0x10C: C0 02 00 00  → 0x2C0

Arithmetic progression suggests array of equal-sized items
or index/directory structure
```

### Cross-Sample Analysis (Multiple Unknown Files)

If you have several files of the same unknown format:

**Common prefix/suffix**
```
diff <(xxd file1.dat | head -20) <(xxd file2.dat | head -20)

Bytes that are IDENTICAL across samples:
  → Magic bytes, version, format constants

Bytes that DIFFER across samples:
  → Variable data, timestamps, sizes, content
```

**Size correlation**
```
File sizes: 1024, 2048, 1536, 3072, 2560

All multiples of 512 → block-aligned format
If file size appears in header → self-describing size field
```

**Field stability analysis**
```
For each offset, compute variance across samples:

Offset 0x00-0x03: variance=0 (constant, probably magic)
Offset 0x04-0x07: variance=low (few values, probably version/type)
Offset 0x08-0x0B: variance=high (different in every file, probably size/checksum)
Offset 0x0C-0x0F: variance=0 (constant, probably reserved/padding)

Low variance = structural, High variance = content
```

### Decompilation Heuristics

When the format might contain code:

**Instruction alignment**
```
x86: variable length, but call/jmp targets often aligned
ARM: 4-byte aligned instructions
WASM: specific section markers (0x00 0x61 0x73 0x6D = "\0asm")

Look for:
  - 0xCC padding (x86 int3)
  - 0x90 runs (x86 nop)
  - Function prologues (push ebp; mov ebp, esp = 55 89 E5)
```

### Practical Blind Analysis Workflow

```
1. FILE SIGNATURE
   - Check first 16 bytes against known signatures
   - Note any ASCII in header

2. ENTROPY MAP
   - Compute sliding window entropy
   - Mark high/low entropy regions
   - Identify likely: header, data, compressed, padding

3. BYTE STATISTICS
   - Byte frequency histogram (single bytes)
   - Bigram frequency analysis (byte pairs)
   - Characterize: text? code? compressed? encrypted?

4. ENDIANNESS PROBE
   - Look for high-byte/low-byte groupings at boundaries
   - Score LE vs BE interpretations of suspected integers
   - Test candidate pointers both ways

5. STRUCTURE SCAN
   - Look for internal pointers (both endianness)
   - Find repeating patterns (autocorrelation)
   - Detect potential TLV structures

6. DATA TYPE SCAN
   - Scan for floats, timestamps, strings
   - Note locations of recognized types
   - Verify pointer targets look reasonable

7. HYPOTHESIZE STRUCTURE
   - Combine all evidence
   - Draw region map with confidence levels
   - Identify "anchor points" of high confidence

8. ITERATE
   - Use anchors to interpret surrounding bytes
   - Refine hypotheses
   - Accept uncertainty where evidence is weak
```

## Differential Analysis Technique

The most powerful technique: **controlled modification**.

```
Scenario: Understanding an image format

1. Create two nearly-identical images
   - Image A: 100x100, red background
   - Image B: 100x100, blue background

2. Hex diff the files
   $ diff <(xxd A.fmt) <(xxd B.fmt)
   → Only bytes at offset 0x24-0x26 differ
   → Hypothesis: RGB color at 0x24

3. Create more variants to confirm
   - Green background: check 0x24-0x26
   - Different size: find width/height fields
   - Add features: find where new data appears

4. Build up understanding incrementally
   - Each experiment confirms or refutes hypotheses
   - Document findings as you go
```

## Confidence Levels

Not all fields are equally understood. Document confidence:

| Level | Meaning | Criteria |
|-------|---------|----------|
| Certain | 100% confident | Verified across all samples, makes semantic sense |
| High | ~90% confident | Consistent pattern, matches expected behavior |
| Medium | ~70% confident | Works for most samples, some ambiguity |
| Low | ~50% confident | Hypothesis, limited verification |
| Unknown | No idea | Bytes exist, meaning unclear |

## Example: Unknown Game Save Format

**Situation**: Game save files with no documentation

```
Turn 1: Collect samples
  - New game save (minimal)
  - Endgame save (maximal)
  - Saves at different points
  - Saves with different player names

Turn 2: Initial analysis
  $(xxd -l 64 save1.dat)
  → Starts with "SAVE" (magic bytes!)
  → Bytes 4-7: different in each file (checksum? size?)
  → Bytes 8-11: same in all files (version?)

Turn 3: Differential - player name
  Create save with name "AAAA" vs "BBBB"
  $(diff <(xxd save_AAAA.dat) <(xxd save_BBBB.dat))
  → Offset 0x100: "AAAA" vs "BBBB" (player name, null-terminated)

Turn 4: Differential - game progress
  Save at level 1 vs level 5
  → Offset 0x50: 01 00 00 00 vs 05 00 00 00 (level, little-endian uint32)

Turn 5: Build parser
  struct SaveFile {
      magic: [u8; 4],      // "SAVE"
      checksum: u32,       // CRC32? (needs verification)
      version: u32,        // Always 0x00000001
      // ...
      level: u32,          // @ 0x50
      // ...
      player_name: String, // @ 0x100, null-terminated
  }

Turn 6: Verify checksum hypothesis
  Modify a byte, check if game rejects file
  → Yes! Confirms checksum field
  → Need to reverse engineer checksum algorithm...
```

## Example: Network Protocol

**Situation**: Proprietary protocol between client/server

```
Turn 1: Capture traffic
  - Record multiple sessions
  - Note what actions caused what packets
  - Label packets with semantic meaning

Turn 2: Find packet structure
  Compare all packets:
  - Common header? (length, type, sequence?)
  - Variable body?
  - Checksum/trailer?

Turn 3: Identify message types
  Action: Login → observe packet pattern
  Action: Send message → observe packet pattern
  → Correlate actions with packet types

Turn 4: Differential on known data
  Send message "hello" vs "world"
  → Find where message text appears
  → Identify length field, encoding

Turn 5: Build protocol spec
  Packet {
      length: u16 (big-endian),
      type: u8,
      sequence: u16,
      payload: [u8; length - 5],
  }

  MessageTypes {
      0x01: Login,
      0x02: Logout,
      0x10: ChatMessage,
      ...
  }
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Encryption | High entropy, no patterns | Look for key material, give up, or find decrypted samples |
| Compression | High entropy but `binwalk` detects | Decompress first, then analyze |
| Wrong endianness | Values don't make sense | Try other endianness |
| Variable-length fields | Parser fails on some files | Add length prefix or delimiter detection |
| Version differences | Parser works on some files | Identify version field, handle variants |

## Anti-patterns

- **Guessing without verification**: Always test hypotheses against multiple samples
- **Ignoring unknown bytes**: Document them even if not understood
- **Over-fitting to one sample**: Format may have optional features
- **Assuming fixed layout**: Many formats have variable-length sections

## Relationship to Code Synthesis

This workflow is almost the **inverse** of code synthesis:

| Code Synthesis | Binary RE |
|----------------|-----------|
| Docs → Code | Binary → Spec |
| Verify code against docs | Verify spec against samples |
| D × C check | Hypothesis × Samples check |
| Hallucination = undocumented code | Hallucination = unverified hypothesis |

Both use exhaustive cross-validation as the core verification strategy.

## Output Artifacts

1. **Format specification** - human-readable documentation
2. **Working parser** - code that reads the format
3. **Test corpus** - samples with known interpretations
4. **Confidence map** - which parts are well-understood vs. uncertain
5. **Open questions** - documented unknowns for future work
