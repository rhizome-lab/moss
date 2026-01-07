# Steganography Detection Workflow

Finding hidden data in files - messages embedded in images, data appended to files, covert channels in protocols.

## Trigger

- Security incident investigation
- Forensic analysis
- Data exfiltration investigation
- CTF challenge
- Compliance audit (data loss prevention)

## Goal

- Detect presence of hidden data
- Extract embedded content if present
- Identify steganographic technique used
- Document findings for investigation

## Prerequisites

- Files to analyze
- Knowledge of steganographic techniques
- Analysis tools
- Understanding of file formats

## Scope

| In Scope | Out of Scope |
|----------|--------------|
| Image steganography | Cryptanalysis of extracted data |
| Audio steganography | Physical steganography |
| File format abuse | Network covert channels |
| Metadata hiding | Encrypted containers |

## Why Detection Is Hard

1. **No obvious markers**: Good stego is statistically similar to normal
2. **Many techniques**: Each requires different detection
3. **False positives**: Compression artifacts look like stego
4. **Adversarial**: Techniques evolve to evade detection
5. **Format complexity**: Deep understanding of formats needed

## Common Steganographic Techniques

### LSB (Least Significant Bit)

```
Original pixel:  RGB(156, 200, 45)
Binary:          10011100, 11001000, 00101101

Hidden bits:     1, 0, 1

Modified pixel:  RGB(157, 200, 45)
Binary:          10011101, 11001000, 00101101
                       ^         ^         ^
                 changed   unchanged  unchanged

Human eye cannot distinguish 156 from 157.
```

### Append/Prepend

```
Normal JPEG:
[SOI marker][image data][EOI marker]

With appended data:
[SOI marker][image data][EOI marker][hidden data]

Image viewers stop at EOI, hidden data ignored.
```

### Metadata Hiding

```
EXIF data in images can contain arbitrary fields.
Comment fields in various formats.
PDF metadata, document properties.
```

### Format-Specific

```
PNG: Hidden in ancillary chunks (tEXt, zTXt, iTXt)
PDF: Incremental updates, object streams
ZIP: Extra fields, file comments
MP3: ID3 tags, between frames
```

## Core Strategy: Triage → Analyze → Extract → Verify

```
┌─────────────────────────────────────────────────────────┐
│                      TRIAGE                              │
│  Quick checks: file size, format validity, metadata     │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     ANALYZE                              │
│  Statistical analysis, visual inspection, format check  │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     EXTRACT                              │
│  Apply suspected technique to retrieve data             │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                      VERIFY                              │
│  Confirm data is meaningful, not noise                  │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Triage

### Basic File Analysis

```bash
# What does file think it is?
file suspicious.jpg
# suspicious.jpg: JPEG image data, JFIF standard 1.01

# File size reasonable for content?
ls -la suspicious.jpg
# If 100KB image of simple shape, suspicious

# Check magic bytes
xxd suspicious.jpg | head -5
# Should start with FF D8 FF for JPEG

# Check file end
xxd suspicious.jpg | tail -10
# Should end with FF D9 for JPEG
```

### Metadata Inspection

```bash
# Extract all metadata
exiftool suspicious.jpg

# Look for unusual fields
exiftool -a -u -g1 suspicious.jpg

# Strings in binary
strings suspicious.jpg | head -50

# Check for embedded files
binwalk suspicious.jpg
```

### Quick Visual Check

```bash
# For images: does it look right?
# - Unusual noise patterns?
# - Areas of static-like texture?
# - Color banding?

# Compare to known clean version if available
compare original.jpg suspicious.jpg diff.png
```

## Phase 2: Statistical Analysis

### LSB Analysis

```python
from PIL import Image
import numpy as np

def analyze_lsb(image_path):
    """Check LSB randomness - stego often changes LSB distribution."""
    img = Image.open(image_path)
    pixels = np.array(img)

    # Extract LSBs
    lsbs = pixels & 1

    # In natural images, LSBs correlate spatially
    # Stego makes them more random
    horizontal_diff = np.abs(lsbs[:, 1:] - lsbs[:, :-1]).mean()
    vertical_diff = np.abs(lsbs[1:, :] - lsbs[:-1, :]).mean()

    print(f"LSB horizontal correlation: {1 - horizontal_diff:.4f}")
    print(f"LSB vertical correlation: {1 - vertical_diff:.4f}")
    # Low correlation = suspicious
```

### Chi-Square Analysis

```python
def chi_square_test(image_path):
    """Detect LSB embedding via chi-square test."""
    img = Image.open(image_path).convert('L')
    pixels = list(img.getdata())

    # Count pairs of values (0,1), (2,3), (4,5), etc.
    # LSB embedding makes pairs equal in frequency
    pairs = {}
    for p in pixels:
        pair = p // 2
        pairs[pair] = pairs.get(pair, [0, 0])
        pairs[pair][p % 2] += 1

    # Chi-square: deviation from expected equal distribution
    chi_sq = 0
    for pair_counts in pairs.values():
        expected = sum(pair_counts) / 2
        if expected > 0:
            chi_sq += sum((c - expected)**2 / expected for c in pair_counts)

    # High chi-square = likely not stego
    # Suspiciously perfect = likely stego
    return chi_sq
```

### Visual Attack

```python
def visual_attack(image_path, output_path):
    """Amplify LSBs for visual inspection."""
    img = Image.open(image_path)
    pixels = np.array(img)

    # Extract LSBs and amplify
    lsbs = (pixels & 1) * 255

    Image.fromarray(lsbs.astype('uint8')).save(output_path)
    # Hidden message may be visible in output
```

### Histogram Analysis

```python
def histogram_attack(image_path):
    """Look for pairs of values with suspiciously equal counts."""
    img = Image.open(image_path).convert('L')
    histogram = img.histogram()

    suspicious_pairs = []
    for i in range(0, 256, 2):
        if histogram[i] > 0 and histogram[i+1] > 0:
            ratio = min(histogram[i], histogram[i+1]) / max(histogram[i], histogram[i+1])
            if ratio > 0.95:  # Suspiciously equal
                suspicious_pairs.append((i, i+1, histogram[i], histogram[i+1]))

    return suspicious_pairs
```

## Phase 3: Format-Specific Checks

### JPEG

```bash
# Check for appended data after EOI
python3 << 'EOF'
with open('suspicious.jpg', 'rb') as f:
    data = f.read()
    eoi = data.rfind(b'\xff\xd9')
    if eoi != -1 and eoi < len(data) - 2:
        print(f"Data after EOI marker: {len(data) - eoi - 2} bytes")
        print(data[eoi+2:eoi+50])  # First 48 bytes
EOF

# JPEG coefficient analysis (DCT steganography)
stegdetect suspicious.jpg
```

### PNG

```bash
# List all chunks
pngcheck -v suspicious.png

# Look for unusual chunks
python3 << 'EOF'
import struct

with open('suspicious.png', 'rb') as f:
    f.read(8)  # Skip signature
    while True:
        header = f.read(8)
        if len(header) < 8:
            break
        length, chunk_type = struct.unpack('>I4s', header)
        print(f"{chunk_type.decode()}: {length} bytes")
        f.read(length + 4)  # Skip data + CRC
EOF
```

### Audio (WAV/MP3)

```bash
# WAV: check for data after RIFF structure
# MP3: check between frames, ID3 tags

sox suspicious.wav -n spectrogram
# Stego may show in spectrogram

# Extract spectral data for analysis
ffprobe -v quiet -show_streams suspicious.mp3
```

### PDF

```bash
# Extract all streams
pdftotext suspicious.pdf output.txt
pdf-parser.py suspicious.pdf

# Look for hidden content
qpdf --show-encryption suspicious.pdf
pdfid.py suspicious.pdf
```

## Phase 4: Extraction

### LSB Extraction

```python
def extract_lsb(image_path, bits_per_channel=1):
    """Extract LSB hidden data."""
    img = Image.open(image_path)
    pixels = list(img.getdata())

    bits = []
    for pixel in pixels:
        for channel in pixel[:3]:  # RGB
            for bit in range(bits_per_channel):
                bits.append((channel >> bit) & 1)

    # Convert bits to bytes
    bytes_data = []
    for i in range(0, len(bits), 8):
        byte = 0
        for bit in bits[i:i+8]:
            byte = (byte << 1) | bit
        bytes_data.append(byte)

    return bytes(bytes_data)
```

### Appended Data Extraction

```bash
# JPEG: extract after EOI
python3 << 'EOF'
with open('suspicious.jpg', 'rb') as f:
    data = f.read()
    eoi = data.rfind(b'\xff\xd9')
    if eoi != -1:
        hidden = data[eoi+2:]
        with open('extracted.bin', 'wb') as out:
            out.write(hidden)
EOF

# Use binwalk to extract embedded files
binwalk -e suspicious.jpg
```

### Tool-Specific Extraction

```bash
# Common tools and their defaults
steghide extract -sf suspicious.jpg
outguess -r suspicious.jpg output.txt
openstego extract -a suspicious.png -sf output.txt

# For CTF, try common passwords
for pw in "" "password" "secret" "123456"; do
    steghide extract -sf image.jpg -p "$pw" 2>/dev/null && echo "Password: $pw"
done
```

## Phase 5: Verification

### Is It Meaningful Data?

```bash
# Check entropy
ent extracted.bin
# High entropy (>7.9) = compressed/encrypted
# Low entropy = plaintext

# Try to identify format
file extracted.bin

# Look for strings
strings extracted.bin

# Check for common file signatures
xxd extracted.bin | head -20
```

### Decrypt If Encrypted

```bash
# Common CTF patterns
# Base64
base64 -d extracted.bin > decoded.bin

# XOR with common keys
python3 -c "
data = open('extracted.bin', 'rb').read()
for key in [0x00, 0xff, 0x42]:
    decoded = bytes(b ^ key for b in data)
    if b'flag' in decoded or decoded.startswith(b'PK'):
        print(f'XOR key: {key}')
        open('decoded.bin', 'wb').write(decoded)
"
```

## Tools Reference

| Tool | Purpose |
|------|---------|
| binwalk | Find embedded files |
| steghide | Common stego tool (extract/embed) |
| stegdetect | Detect JPEG stego |
| zsteg | PNG/BMP stego analysis |
| stegsolve | Visual image analysis (GUI) |
| exiftool | Metadata extraction |
| outguess | Another stego tool |
| openstego | Cross-platform stego |
| pngcheck | PNG format validation |
| foremost | Carve files from data |

## LLM-Specific Techniques

### Technique Identification

```
Given this file analysis:

- JPEG image, 847KB
- Simple photograph of a landscape
- exiftool shows unusual "Comment" field with base64 string
- binwalk reports: "JPEG image data" at offset 0, nothing else
- LSB analysis shows normal correlation
- Data after EOI marker: 0 bytes

What steganographic techniques should I investigate?
What extraction methods should I try?
```

### Pattern Recognition

```
I extracted this data from image LSBs:
[hex dump or sample]

Help identify:
1. Is this meaningful data or noise?
2. If meaningful, what format/encoding?
3. What decoding steps should I try?
```

### Format Analysis

```
This PNG has these chunks:
- IHDR (13 bytes)
- tEXt (45 bytes, key="Comment")
- IDAT (23456 bytes)
- iTXt (1024 bytes, key="XML:com.adobe.xmp")
- IEND (0 bytes)

Which chunks are unusual? What should I examine?
```

## Common CTF Patterns

### Quick Wins

```bash
# Always try first
strings image.png | grep -i flag
exiftool image.png | grep -i flag
binwalk -e image.png

# Check file size vs dimensions
identify image.png  # Expected size for dimensions?

# Try default steghide (no password)
steghide extract -sf image.jpg -p ""
```

### Multi-Layer

```
Image → steghide → zip → base64 → flag

Common pattern:
1. Extract with steghide
2. Result is a zip
3. Unzip (might be password protected)
4. Decode inner file (base64, hex, etc.)
5. Get flag
```

### Audio Spectrogram

```bash
# Hidden message visible in spectrogram
sox suspicious.wav -n spectrogram -o spectrogram.png
# Open PNG, look for text/images
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| False positive | Extraction yields noise | Try different techniques |
| Wrong technique | Tool fails/garbage output | Systematic technique enumeration |
| Encrypted payload | High entropy, no structure | Need password/key |
| Unknown format | File unrecognized | Manual format analysis |

## Anti-patterns

- **Assuming one technique**: May be layered/combined
- **Trusting file extension**: Extension often lies
- **Stopping at first find**: May be decoy hiding real data
- **Ignoring metadata**: Often simplest hiding place
- **Not checking "normal" files**: Clean-looking files may be stego carriers

## Detection Limitations

Steganography detection is fundamentally limited:

1. **Perfect security**: Well-designed stego is statistically indistinguishable
2. **No universal detector**: Each technique needs specific analysis
3. **Cover traffic**: If attacker controls many files, hard to find the carrier
4. **Deniability**: Noise might actually be noise

## Open Questions

### Automated Detection

Can ML detect unknown stego techniques?
- Research area (steganalysis with CNNs)
- Works for known techniques
- Unknown/adaptive stego still challenging

### Deep Learning Stego

Neural network-based stego is emerging:
- Steganographic GANs generate carrier images
- No statistical artifacts of traditional stego
- Detection requires similar ML approaches

### Forensic Standards

When is stego detection admissible evidence?
- Statistical evidence can be challenged
- False positive rates matter
- Need reproducible methodology

## See Also

- [Reverse Engineering Binary](reverse-engineering-binary.md) - File format analysis
- [Security Audit](security-audit.md) - Broader security context
- [Cryptanalysis](cryptanalysis.md) - If extracted data is encrypted

