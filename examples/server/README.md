# Moss FastAPI Server

A production-ready REST API server for Moss functionality.

## Installation

Install with server dependencies:

```bash
uv pip install -e ".[server]"
```

## Running

Start the server:

```bash
uvicorn examples.server.main:app --reload
```

Or run directly:

```bash
python -m examples.server.main
```

The API will be available at http://localhost:8000

## API Documentation

Interactive documentation is available at:
- Swagger UI: http://localhost:8000/docs
- ReDoc: http://localhost:8000/redoc

## Endpoints

### GET /
API root with version info.

### GET /health
Health check endpoint.

### POST /skeleton
Extract Python skeleton from source code.

```bash
curl -X POST http://localhost:8000/skeleton \
  -H "Content-Type: application/json" \
  -d '{"source": "def foo(): pass\nclass Bar: pass"}'
```

### POST /anchors
Find anchors (functions, classes, methods) in source code.

```bash
curl -X POST http://localhost:8000/anchors \
  -H "Content-Type: application/json" \
  -d '{"source": "def foo(): pass", "anchor_type": "function", "name": "foo"}'
```

### POST /patch
Apply AST-based patch to source code.

```bash
curl -X POST http://localhost:8000/patch \
  -H "Content-Type: application/json" \
  -d '{
    "source": "def foo(): pass",
    "anchor_type": "function",
    "anchor_name": "foo",
    "patch_type": "replace",
    "content": "def foo(): return 42"
  }'
```

### POST /patch/text
Apply text-based search-and-replace patch.

```bash
curl -X POST http://localhost:8000/patch/text \
  -H "Content-Type: application/json" \
  -d '{
    "source": "def foo(): pass",
    "old_text": "pass",
    "new_text": "return 42"
  }'
```

### POST /cfg
Build control flow graph from source code.

```bash
curl -X POST http://localhost:8000/cfg \
  -H "Content-Type: application/json" \
  -d '{"source": "def foo(x):\n  if x: return 1\n  return 0"}'
```

### POST /elide
Elide literals from source code to reduce token count.

```bash
curl -X POST http://localhost:8000/elide \
  -H "Content-Type: application/json" \
  -d '{"source": "x = \"hello world\"\ny = 12345"}'
```

### POST /validate
Validate Python source code syntax.

```bash
curl -X POST http://localhost:8000/validate \
  -H "Content-Type: application/json" \
  -d '{"source": "def foo(): pass"}'
```
