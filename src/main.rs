use bytes::{Buf, BufMut, BytesMut};
use color_eyre::eyre::eyre;
use nom::bytes::streaming::{tag, take_until};
use nom::combinator::{consumed, map, peek};
use nom::error::ErrorKind;
use nom::multi::many0;
use nom::IResult;
use smallstring::SmallString;
use std::fmt::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let mut listener = TcpListener::bind("0.0.0.0:1337").await?;
    while let (stream, _) = listener.accept().await? {
        tokio::spawn(async move {
            if let Err(err) = handle_con(stream).await {
                eprintln!("{err}");
            }
        });
    }

    Ok(())
}

async fn handle_con(mut stream: TcpStream) -> color_eyre::Result<()> {
    let mut buf = BytesMut::with_capacity(1024);
    let mut resp_buf = BytesMut::with_capacity(1024);

    loop {
        if 0 == stream.read_buf(&mut buf).await? {
            // if we get to reading again the buffer must be empty or the connection closed,
            // its safe to return without breaking connection state
            return Ok(());
        }

        loop {
            let (l, req_url) = match read_request_len(&mut buf) {
                Ok((_, lu)) => lu,
                Err(nom::Err::Incomplete(_)) => break,
                Err(nom::Err::Error(err)) => return Err(eyre!("protocol error: {err:?}")),
                Err(nom::Err::Failure(err)) => return Err(eyre!("protocol error: {err:?}")),
            };

            write!(
                &mut resp_buf,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                req_url.len()
            )?;
            resp_buf.put_slice(req_url);
            buf.advance(l);
        }

        stream.write_all_buf(&mut resp_buf).await?;
        stream.flush().await?;
        resp_buf.clear();
    }
}

fn read_request_len(i: &[u8]) -> IResult<&[u8], (usize, &[u8])> {
    map(consumed(read_request), |(l, u)| (l.len(), u))(i)
}

fn read_request(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let (i, url) = read_request_line(i)?;
    let (i, _) = headers(i)?;
    let (i, _) = tag("\r\n")(i)?;

    Ok((i, url))
}

fn read_request_line(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let (i, _) = tag(b"GET ")(i)?;
    let (i, url) = take_until(" ")(i)?;
    let (i, _) = tag(b" HTTP/1.1\r\n")(i)?;

    Ok((
        i,
        url
    ))
}

// skip over headers lol
fn headers(i: &[u8]) -> IResult<&[u8], ()> {
    map(many0(header), drop)(i)
}

fn header(i: &[u8]) -> IResult<&[u8], ()> {
    let peek_res: IResult<_, _> = peek(tag("\r\n"))(i);
    if peek_res.is_ok() {
        return Err(nom::Err::Error(nom::error::Error::new(
            i,
            ErrorKind::TakeUntil,
        )));
    }
    let (i, _) = take_until(":")(i)?;
    let (i, _) = tag(": ")(i)?;
    let (i, _) = take_until("\r")(i)?;
    map(tag("\r\n"), drop)(i)
}
