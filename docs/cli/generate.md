# moss generate

Generate code from API specifications.

## Usage

```bash
moss generate <SPEC> [OPTIONS]
```

## Examples

```bash
# From OpenAPI spec
moss generate openapi.yaml --output src/api/

# From GraphQL schema
moss generate schema.graphql --lang typescript
```

## Supported Formats

| Format | Extensions |
|--------|------------|
| OpenAPI | `.yaml`, `.json` |
| GraphQL | `.graphql`, `.gql` |
| Protobuf | `.proto` |

## Options

- `--output <DIR>` - Output directory
- `--lang <LANG>` - Target language
- `--dry-run` - Show what would be generated
