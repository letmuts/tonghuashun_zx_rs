/// 简易 Protobuf varint 编解码工具。
///
/// 用于 multiStorage blockstock 协议的二进制载荷构造与解析。
/// 实现了 Protocol Buffers 的 varint 编码、tag 编码、length-delimited 字段编码。

/// Wire type: varint。
const WIRETYPE_VARINT: u64 = 0;

/// Wire type: length-delimited。
const WIRETYPE_LENGTH_DELIMITED: u64 = 2;

/// 将无符号整数编码为 Protocol Buffers varint 格式的字节。
///
/// 每个字节的低 7 位存储数据，最高位表示是否继续。
pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    while value > 127 {
        buf.push(((value & 0x7F) as u8) | 0x80);
        value >>= 7;
    }
    buf.push((value & 0x7F) as u8);
    buf
}

/// 从字节数组中解码一个 varint。
///
/// # Arguments
/// * `data` - 原始字节数据。
/// * `offset` - 起始偏移量。
///
/// # Returns
/// `(解码出的值, 新的偏移量)`
pub fn decode_varint(data: &[u8], mut offset: usize) -> (u64, usize) {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    while offset < data.len() {
        let byte = data[offset];
        value |= ((byte & 0x7F) as u64) << shift;
        offset += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    (value, offset)
}

/// 构造一个 varint 类型的 protobuf 字段。
///
/// 格式：`tag + value`，其中 tag = `(field_number << 3) | wire_type`。
pub fn field_varint(field_number: u64, value: u64) -> Vec<u8> {
    let tag = (field_number << 3) | WIRETYPE_VARINT;
    let mut buf = encode_varint(tag);
    buf.extend(encode_varint(value));
    buf
}

/// 构造一个 length-delimited 类型的 protobuf 字段。
///
/// 格式：`tag + length + payload`。
pub fn field_bytes(field_number: u64, payload: &[u8]) -> Vec<u8> {
    let tag = (field_number << 3) | WIRETYPE_LENGTH_DELIMITED;
    let mut buf = encode_varint(tag);
    buf.extend(encode_varint(payload.len() as u64));
    buf.extend_from_slice(payload);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_varint_small() {
        assert_eq!(encode_varint(1), vec![0x01]);
        assert_eq!(encode_varint(127), vec![0x7F]);
    }

    #[test]
    fn test_encode_varint_large() {
        assert_eq!(encode_varint(128), vec![0x80, 0x01]);
        assert_eq!(encode_varint(300), vec![0xAC, 0x02]);
    }

    #[test]
    fn test_decode_varint() {
        assert_eq!(decode_varint(&[0x01], 0), (1, 1));
        assert_eq!(decode_varint(&[0x80, 0x01], 0), (128, 2));
        assert_eq!(decode_varint(&[0xAC, 0x02], 0), (300, 2));
    }

    #[test]
    fn test_field_varint() {
        let result = field_varint(1, 150);
        // tag: 1<<3|0 = 8 = 0x08, value 150 = 0x96 0x01
        assert_eq!(result, vec![0x08, 0x96, 0x01]);
    }

    #[test]
    fn test_field_bytes() {
        let result = field_bytes(1, b"hello");
        // tag: 1<<3|2 = 10 = 0x0A, len=5, payload=b"hello"
        assert_eq!(result.len(), 1 + 1 + 5);
        assert_eq!(result[0], 0x0A);
        assert_eq!(result[1], 0x05);
        assert_eq!(&result[2..], b"hello");
    }
}
