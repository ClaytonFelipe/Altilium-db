use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take},
    character::complete::{crlf, i64},
    multi::count,
    sequence::{preceded, terminated},
    IResult,
};
use std::string::FromUtf8Error;

#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Vec<u8>),
    Array(Vec<RespValue>),
    Null,
}

impl RespValue {
    
    pub fn to_string(self) -> Result<String, FromUtf8Error> {
        match self {
            RespValue::BulkString(bytes) => String::from_utf8(bytes),
            RespValue::SimpleString(s) => Ok(s),
            _ => Err(String::from_utf8(vec![]).unwrap_err()),
        }
    }
}


pub fn parse_resp(input: &[u8]) -> IResult<&[u8], RespValue> {
    alt((
        parse_simple_string,
        parse_error,
        parse_integer,
        parse_bulk_string,
        parse_array,
    ))(input)
}

fn parse_simple_string(input: &[u8]) -> IResult<&[u8], RespValue> {
    let (input, content) = preceded(tag("+"), terminated(is_not("\r\n"), crlf))(input)?;
    Ok((
        input,
        RespValue::SimpleString(String::from_utf8_lossy(content).to_string()),
    ))
}

fn parse_error(input: &[u8]) -> IResult<&[u8], RespValue> {
    let (input, content) = preceded(tag("-"), terminated(is_not("\r\n"), crlf))(input)?;
    Ok((
        input,
        RespValue::Error(String::from_utf8_lossy(content).to_string()),
    ))
}

fn parse_integer(input: &[u8]) -> IResult<&[u8], RespValue> {
    let (input, number) = preceded(tag(":"), terminated(i64, crlf))(input)?;
    Ok((input, RespValue::Integer(number)))
}

fn parse_bulk_string(input: &[u8]) -> IResult<&[u8], RespValue> {
    
    let (input, len_i64) = preceded(tag("$"), terminated(i64, crlf))(input)?;
    let len = len_i64 as isize;

    if len == -1 {
        return Ok((input, RespValue::Null));
    }

    let (input, data) = terminated(take(len as usize), crlf)(input)?;
    Ok((input, RespValue::BulkString(data.to_vec())))
}

fn parse_array(input: &[u8]) -> IResult<&[u8], RespValue> {
    
    let (input, len_i64) = preceded(tag("*"), terminated(i64, crlf))(input)?;
    let len = len_i64 as isize;

    if len == -1 {
        return Ok((input, RespValue::Null));
    }

    let (input, elements) = count(parse_resp, len as usize)(input)?;
    Ok((input, RespValue::Array(elements)))
}


pub fn serialize_resp(value: RespValue) -> Vec<u8> {
    match value {
        RespValue::SimpleString(s) => format!("+{}\r\n", s).into_bytes(),
        RespValue::Error(s) => format!("-{}\r\n", s).into_bytes(),
        RespValue::Integer(i) => format!(":{}\r\n", i).into_bytes(),
        RespValue::BulkString(bytes) => {
            let mut result = format!("${}\r\n", bytes.len()).into_bytes();
            result.extend(&bytes);
            result.extend_from_slice(b"\r\n");
            result
        }
        RespValue::Array(arr) => {
            let mut result = format!("*{}\r\n", arr.len()).into_bytes();
            for val in arr {
                result.extend(serialize_resp(val));
            }
            result
        }
        RespValue::Null => b"$-1\r\n".to_vec(),
    }
}