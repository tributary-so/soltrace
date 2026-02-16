use crate::error::{Result, SoltraceError};
use crate::types::IdlField;
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;

/// Decodes borsh-serialized data based on IDL field definitions
pub struct BorshDecoder;

impl BorshDecoder {
    /// Decode borsh data into a JSON object using IDL field definitions
    pub fn decode_event_data(data: &[u8], fields: &[IdlField]) -> Result<Value> {
        let mut result = serde_json::Map::new();
        let mut offset = 0;

        for field in fields {
            let (value, bytes_read) = Self::decode_field(data, offset, &field.field_type)?;
            result.insert(field.name.clone(), value);
            offset += bytes_read;
        }

        if offset != data.len() {
            return Err(SoltraceError::EventDecode(format!(
                "Data length mismatch: decoded {} bytes, but data is {} bytes",
                offset,
                data.len()
            )));
        }

        Ok(Value::Object(result))
    }

    /// Decode a single field and return (value, bytes_read)
    fn decode_field(data: &[u8], offset: usize, field_type: &str) -> Result<(Value, usize)> {
        let data = &data[offset..];

        match field_type {
            // Boolean
            "bool" => {
                if data.is_empty() {
                    return Err(SoltraceError::EventDecode(
                        "Unexpected end of data".to_string(),
                    ));
                }
                Ok((Value::Bool(data[0] != 0), 1))
            }

            // Unsigned integers
            "u8" => Self::decode_integer::<u8>(data, 1).map(|(v, n)| (Value::Number(v.into()), n)),
            "u16" => {
                Self::decode_integer::<u16>(data, 2).map(|(v, n)| (Value::Number(v.into()), n))
            }
            "u32" => {
                Self::decode_integer::<u32>(data, 4).map(|(v, n)| (Value::Number(v.into()), n))
            }
            "u64" => {
                let (v, n) = Self::decode_integer::<u64>(data, 8)?;
                Ok((Value::String(v.to_string()), n))
            }
            "u128" => {
                let (v, n) = Self::decode_u128(data)?;
                Ok((Value::String(v.to_string()), n))
            }

            // Signed integers
            "i8" => Self::decode_integer::<i8>(data, 1).map(|(v, n)| (Value::Number(v.into()), n)),
            "i16" => {
                Self::decode_integer::<i16>(data, 2).map(|(v, n)| (Value::Number(v.into()), n))
            }
            "i32" => {
                Self::decode_integer::<i32>(data, 4).map(|(v, n)| (Value::Number(v.into()), n))
            }
            "i64" => {
                let (v, n) = Self::decode_integer::<i64>(data, 8)?;
                Ok((Value::String(v.to_string()), n))
            }
            "i128" => {
                let (v, n) = Self::decode_i128(data)?;
                Ok((Value::String(v.to_string()), n))
            }

            // String
            "string" => {
                let (s, n) = Self::decode_string(data)?;
                Ok((Value::String(s), n))
            }

            // PublicKey
            "publicKey" | "pubkey" | "Pubkey" => {
                if data.len() < 32 {
                    return Err(SoltraceError::EventDecode(
                        "Not enough data for Pubkey".to_string(),
                    ));
                }
                let pubkey = Pubkey::try_from(&data[..32])
                    .map_err(|e| SoltraceError::EventDecode(format!("Invalid pubkey: {}", e)))?;
                Ok((Value::String(pubkey.to_string()), 32))
            }

            // Byte arrays
            "bytes" => {
                let (bytes, n) = Self::decode_bytes(data)?;
                Ok((Value::String(hex::encode(&bytes)), n))
            }

            // Option<T>
            t if t.starts_with("option<") && t.ends_with(">") => {
                if data.is_empty() {
                    return Err(SoltraceError::EventDecode(
                        "Unexpected end of data".to_string(),
                    ));
                }
                let is_some = data[0] != 0;
                if is_some {
                    let inner_type = &t[7..t.len() - 1];
                    let (value, bytes_read) = Self::decode_field(&data[1..], 0, inner_type)?;
                    Ok((value, 1 + bytes_read))
                } else {
                    Ok((Value::Null, 1))
                }
            }

            // Vec<T>
            t if t.starts_with("vec<") && t.ends_with(">") => {
                let inner_type = &t[4..t.len() - 1];
                let (arr, bytes_read) = Self::decode_vec(data, inner_type)?;
                Ok((Value::Array(arr), bytes_read))
            }

            // Array [T; N]
            t if t.starts_with('[') && t.contains(";") => {
                let parts: Vec<&str> = t[1..t.len() - 1].split(';').collect();
                if parts.len() != 2 {
                    return Err(SoltraceError::EventDecode(format!(
                        "Invalid array type: {}",
                        t
                    )));
                }
                let inner_type = parts[0].trim();
                let len: usize = parts[1].trim().parse().map_err(|_| {
                    SoltraceError::EventDecode(format!("Invalid array length: {}", parts[1]))
                })?;

                let mut arr = Vec::with_capacity(len);
                let mut total_bytes = 0;
                for _ in 0..len {
                    let (value, bytes_read) =
                        Self::decode_field(&data[total_bytes..], 0, inner_type)?;
                    arr.push(value);
                    total_bytes += bytes_read;
                }
                Ok((Value::Array(arr), total_bytes))
            }

            // Unknown type
            _ => Err(SoltraceError::EventDecode(format!(
                "Unsupported field type: {}. Consider using hex encoding.",
                field_type
            ))),
        }
    }

    /// Decode a little-endian integer
    fn decode_integer<T: TryFrom<u128>>(data: &[u8], size: usize) -> Result<(T, usize)> {
        if data.len() < size {
            return Err(SoltraceError::EventDecode(
                "Not enough data for integer".to_string(),
            ));
        }

        let mut bytes = [0u8; 16];
        bytes[..size].copy_from_slice(&data[..size]);
        let value = u128::from_le_bytes(bytes);

        T::try_from(value)
            .map(|v| (v, size))
            .map_err(|_| SoltraceError::EventDecode("Integer conversion failed".to_string()))
    }

    /// Decode u128 (always returns as string to avoid precision issues)
    fn decode_u128(data: &[u8]) -> Result<(u128, usize)> {
        if data.len() < 16 {
            return Err(SoltraceError::EventDecode(
                "Not enough data for u128".to_string(),
            ));
        }
        let bytes: [u8; 16] = data[..16].try_into().unwrap();
        Ok((u128::from_le_bytes(bytes), 16))
    }

    /// Decode i128 (always returns as string to avoid precision issues)
    fn decode_i128(data: &[u8]) -> Result<(i128, usize)> {
        if data.len() < 16 {
            return Err(SoltraceError::EventDecode(
                "Not enough data for i128".to_string(),
            ));
        }
        let bytes: [u8; 16] = data[..16].try_into().unwrap();
        Ok((i128::from_le_bytes(bytes), 16))
    }

    /// Decode a borsh string (4-byte length prefix + content)
    fn decode_string(data: &[u8]) -> Result<(String, usize)> {
        if data.len() < 4 {
            return Err(SoltraceError::EventDecode(
                "Not enough data for string length".to_string(),
            ));
        }

        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if data.len() < 4 + len {
            return Err(SoltraceError::EventDecode(
                "Not enough data for string content".to_string(),
            ));
        }

        let s = String::from_utf8(data[4..4 + len].to_vec())
            .map_err(|e| SoltraceError::EventDecode(format!("Invalid UTF-8: {}", e)))?;

        Ok((s, 4 + len))
    }

    /// Decode borsh bytes (4-byte length prefix + content)
    fn decode_bytes(data: &[u8]) -> Result<(Vec<u8>, usize)> {
        if data.len() < 4 {
            return Err(SoltraceError::EventDecode(
                "Not enough data for bytes length".to_string(),
            ));
        }

        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if data.len() < 4 + len {
            return Err(SoltraceError::EventDecode(
                "Not enough data for bytes content".to_string(),
            ));
        }

        Ok((data[4..4 + len].to_vec(), 4 + len))
    }

    /// Decode a vector of elements
    fn decode_vec(data: &[u8], inner_type: &str) -> Result<(Vec<Value>, usize)> {
        if data.len() < 4 {
            return Err(SoltraceError::EventDecode(
                "Not enough data for vec length".to_string(),
            ));
        }

        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let mut result = Vec::with_capacity(len);
        let mut total_bytes = 4;

        for _ in 0..len {
            let (value, bytes_read) = Self::decode_field(&data[total_bytes..], 0, inner_type)?;
            result.push(value);
            total_bytes += bytes_read;
        }

        Ok((result, total_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IdlField;

    #[test]
    fn test_decode_u64() {
        let data = 42u64.to_le_bytes().to_vec();
        let fields = vec![IdlField {
            name: "amount".to_string(),
            field_type: "u64".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert_eq!(result["amount"], "42");
    }

    #[test]
    fn test_decode_pubkey() {
        let pubkey = Pubkey::new_unique();
        let data = pubkey.to_bytes().to_vec();
        let fields = vec![IdlField {
            name: "owner".to_string(),
            field_type: "publicKey".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert_eq!(result["owner"], pubkey.to_string());
    }

    #[test]
    fn test_decode_string() {
        let s = "Hello, World!";
        let mut data = (s.len() as u32).to_le_bytes().to_vec();
        data.extend_from_slice(s.as_bytes());

        let fields = vec![IdlField {
            name: "message".to_string(),
            field_type: "string".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert_eq!(result["message"], s);
    }

    #[test]
    fn test_decode_bool() {
        let data = vec![1u8]; // true
        let fields = vec![IdlField {
            name: "active".to_string(),
            field_type: "bool".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert_eq!(result["active"], true);
    }

    #[test]
    fn test_decode_multiple_fields() {
        // Build data for: { amount: u64, owner: Pubkey }
        let amount = 1000u64;
        let owner = Pubkey::new_unique();

        let mut data = amount.to_le_bytes().to_vec();
        data.extend_from_slice(&owner.to_bytes());

        let fields = vec![
            IdlField {
                name: "amount".to_string(),
                field_type: "u64".to_string(),
            },
            IdlField {
                name: "owner".to_string(),
                field_type: "publicKey".to_string(),
            },
        ];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert_eq!(result["amount"], "1000");
        assert_eq!(result["owner"], owner.to_string());
    }

    #[test]
    fn test_decode_vec() {
        // vec<u8> with 3 elements: [1, 2, 3]
        let mut data = 3u32.to_le_bytes().to_vec(); // length
        data.extend_from_slice(&[1u8, 2u8, 3u8]);

        let fields = vec![IdlField {
            name: "data".to_string(),
            field_type: "vec<u8>".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert!(result["data"].is_array());
        assert_eq!(result["data"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_decode_option_some() {
        // option<u64> with Some(42)
        let mut data = vec![1u8]; // is_some = true
        data.extend_from_slice(&42u64.to_le_bytes());

        let fields = vec![IdlField {
            name: "value".to_string(),
            field_type: "option<u64>".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert_eq!(result["value"], "42");
    }

    #[test]
    fn test_decode_option_none() {
        // option<u64> with None
        let data = vec![0u8]; // is_some = false

        let fields = vec![IdlField {
            name: "value".to_string(),
            field_type: "option<u64>".to_string(),
        }];

        let result = BorshDecoder::decode_event_data(&data, &fields).unwrap();
        assert!(result["value"].is_null());
    }
}
