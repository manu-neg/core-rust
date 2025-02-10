use std::time::Duration;
use async_std::prelude::*;
use async_std::net::{TcpListener, TcpStream};
use async_std::io::{BufReader, WriteExt};
use async_std::fs;
use async_std::sync::{Arc, RwLock};
use async_std::task::sleep;
use opencv::{core::Vector, imgcodecs, prelude::*};
/*
use image;
use std::io::Cursor;
*/
pub type TransmissionType = Arc<RwLock<Option<Vec<u8>>>>;


pub async fn tcp_async(host: &str, port: &str, pipe: TransmissionType) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("{}:{}", host, port)).await?;
    println!("Tcp server is up");
    loop {

        if let Ok((conn, _addr)) = listener.accept().await {

            // println!("[+] Incoming from {}", &_addr); debug
            if let Err(e) = handle_connection(conn, Arc::clone(&pipe)).await {

                eprintln!("Thread failed due to {}. Restarting", e.kind());
            }
        }
    }
}

fn vec_to_jpeg(raw_bytes: Vec<u8>) -> opencv::Result<Mat> {
    let data = Vector::from_slice(&raw_bytes); // Create Mat from raw bytes
    let result = imgcodecs::imdecode(&data, imgcodecs::IMREAD_COLOR)?;
    Ok(result)
}


async fn handle_connection(mut s: TcpStream, pipe: TransmissionType) -> std::io::Result<()> {
    let mut seg: [u8; 8] = [0; 8];
    let mut req: Vec<u8>;
    loop {

        s.read_exact(&mut seg).await?;
        let content_size: usize = u64::from_be_bytes(seg) as usize;
        println!("content-size: {}", &content_size);
        seg = [0; 8];
        req = vec![0u8; content_size];
        s.read_exact(&mut req).await?; 
        // comentar esto al habilitar el siguiente bloque
        let mut queue = pipe.write().await;
        (*queue) = Some(req);
        // ESTE BLOQUE ES PARA PROCESAR LAS IMAGENES
        // reduce la velocidad

        /*
        match vec_to_jpeg(req) {
            Ok(img) => {
                println!("Img conversion good");
                imgcodecs::imwrite("Images/test.jpeg", &img, &Vector::new()).expect("Failed to write test");


            },
            Err(e) => {
                eprintln!("{}", e);
            }
        }
        */
    }
}

async fn handle_http_connection(mut s: TcpStream) -> std::io::Result<()> {
    let mut bufreader = BufReader::new(&mut s);
    let mut buf: [u8; 1024] = [0u8; 1024];
    
    bufreader.read(&mut buf).await.unwrap();
    // si no logra leer bytes debe fallar.
    let content_name: &str;
    let is_image: bool;

    if buf.starts_with(b"GET /Images/img.jpg HTTP/1.1") {
        (content_name, is_image) = ("./Images/img.jpg", true);

    } else if buf.starts_with(b"GET / HTTP/1.1") {
        (content_name, is_image) = ("content.html", false);

    } else {
        (content_name, is_image) = ("404.html", false);

    }

    if is_image {

        let html_content = fs::read(content_name).await.unwrap_or(vec![0u8; 1024]);
        let length = html_content.len();
        s.write_all(format!("HTTP/1.1 200 OK\r\nContent-length: {length}\r\nContent-type: image/jpeg\r\n\r\n").as_bytes()).await?;
        s.write_all(&html_content).await?;

    } else {

        let html_content = fs::read_to_string(content_name).await.unwrap_or(String::new());
        let length = html_content.len();
        s.write_all(format!("HTTP/1.1 200 OK\r\nContent-length: {length}\r\n\r\n{html_content}").as_bytes()).await?;

    }
    return Ok(());
}

pub async fn http_camera_feed(host: &str, port: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("{}:{}", host, port)).await?;
    println!("Http server is up");
    loop {

        if let Ok((conn, _addr)) = listener.accept().await {

            // println!("[+] Incoming from {}", &_addr);
            if let Err(e) = handle_http_connection(conn).await {

                eprintln!("Thread failed due to {}. Restarting", e.kind());
            }
        }
    }
}

async fn handle_mjpeg_connection(mut s: TcpStream, pipe: TransmissionType) -> std::io::Result<()> {
    let mut bufreader = BufReader::new(&mut s);
    let mut buf: [u8; 1024] = [0u8; 1024];
    bufreader.read(&mut buf).await.unwrap();
    // si no logra leer bytes debe fallar.
/*
MJPEG STREAM SPECS

1- http 200 ok + headers and boundary specification + content type
2- --boundary + CRLF + headers + CRLFCRLF
frame + crlf

repeat 2

*/

    let mut image_clone: Vec<u8> = Vec::new();
    let initial_headers = "HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=end\r\n\r\n";
    s.write_all(&initial_headers.as_bytes()).await?;
    let mut partial_headers: String;
    loop {

        if let Some(i) = (*(pipe.read().await)).clone() {
            if image_clone == i {
                sleep(Duration::from_millis(10)).await;
                continue;
            } else {
                image_clone = i;
            }
        } else {
            sleep(Duration::from_millis(10)).await;
            continue;
        }
        partial_headers = format!("--end\r\nContent-Length:{}\r\nContent-Type: image/jpeg\r\n\r\n", &image_clone.len());

        match write_to_stream(&mut s, &partial_headers, &image_clone).await {
            Ok(()) => {},
            Err(e) => {
                eprintln!("Connection terminated. Error: {}", e);
                break;
            }
        }
    }

    return Ok(());
}

async fn write_to_stream(s: &mut TcpStream, partial_headers: &String, image_clone: &Vec<u8>) -> std::io::Result<()> {

    s.write(partial_headers.as_bytes()).await?;
    s.write(image_clone).await?;
    s.write(b"\r\n\r\n").await?;
    s.flush().await?;
    Ok(())
}

pub async fn mjpeg_stream(host: &str, port: &str, pipe: TransmissionType) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("{}:{}", host, port)).await?;
    println!("MJPEG server is up");
    loop {

        if let Ok((conn, addr)) = listener.accept().await {

            println!("[+] Incoming from {}", &addr);
            if let Err(e) = handle_mjpeg_connection(conn, Arc::clone(&pipe)).await {

                eprintln!("Thread failed due to {}. Restarting", e.kind());
            }
        }
    }
}