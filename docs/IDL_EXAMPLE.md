# Example Anchor IDL for Testing

This IDL demonstrates a simple Solana program with events. Use this for testing Soltrace.

## IDL Structure

```json
{
  "version": "0.31.1",
  "name": "example_program",
  "address": "ExamPleProgramAddress111111111111111111111",
  "events": [
    {
      "name": "Transfer",
      "fields": [
        {
          "name": "from",
          "type": "publicKey"
        },
        {
          "name": "to",
          "type": "publicKey"
        },
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    },
    {
      "name": "Mint",
      "fields": [
        {
          "name": "to",
          "type": "publicKey"
        },
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    }
  ]
}
```

## Usage

1. Copy the above JSON to a file named `example_program.json`
2. Place it in the `idls/` directory
3. Run the indexer with the program address

## Notes

- The address in the IDL should match the actual deployed program ID
- Events define the structure of emitted events
- The discriminator is calculated as SHA256("event:<name>")[:8]

## Testing

You can test the IDL parsing with:

```rust
use soltrace_core::idl::IdlParser;

#[tokio::main]
async fn main() -> Result<()> {
    let mut parser = IdlParser::new();
    parser.load_from_file("idls/example_program.json").await?;

    let events = parser.get_events("ExamPleProgramAddress111111111111111111111").unwrap();
    println!("Loaded {} events", events.len());

    Ok(())
}
```
