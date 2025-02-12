use std::time::{Duration, Instant};
use async_std::prelude::*;
use async_std::net::{TcpListener, TcpStream};
use async_std::io::{BufReader, WriteExt};
use async_std::fs;
use async_std::sync::{Arc, RwLock};
use async_std::task::sleep;
use opencv::{core::{Mat, Size, Vector}, imgcodecs, prelude::*};
use opencv::videoio::VideoWriter;

use crate::asyncrs::detector::{get_classifier_model, process_person_detection, ReturnMetadata};

/*
use ffmpeg_next::codec::{Id, encoder, Context};
use ffmpeg_next::format::{output, Pixel};
use ffmpeg_next::Packet;
 */
use chrono::Local;

// use std::time::Instant;

pub type TransmissionType = Arc<RwLock<Option<Vec<u8>>>>;

const RECORDING_STOP_INTERVAL: Duration = Duration::from_secs(5);
const FALSE_POSITIVE_FRAME_COUNT: usize = 5;

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
#[allow(dead_code)]
fn vec_to_jpeg(raw_bytes: Vec<u8>) -> opencv::Result<Mat> {
    let data = Vector::from_slice(&raw_bytes); // Create Mat from raw bytes
    let result = imgcodecs::imdecode(&data, imgcodecs::IMREAD_COLOR)?;
    Ok(result)
}


fn init_video_writer(file_name: &str, width: i32, height: i32, fps: f64) -> opencv::Result<VideoWriter> {

    let fourcc = VideoWriter::fourcc('M', 'J', 'P', 'G')?; // MJPEG codec (you can choose others like H264 or XVID)
    let frame_size = Size::new(width, height);
    let video_writer = VideoWriter::new(file_name, fourcc, fps, frame_size, true)?;
    return Ok(video_writer);

}





async fn handle_connection(mut s: TcpStream, pipe: TransmissionType) -> std::io::Result<()> {
    let mut seg: [u8; 8] = [0; 8];
    
    //detection accumulative result avoid false positive
    let mut buffer1: [bool; FALSE_POSITIVE_FRAME_COUNT] = [false; FALSE_POSITIVE_FRAME_COUNT];
    let mut buffer2: [bool; FALSE_POSITIVE_FRAME_COUNT] = [false; FALSE_POSITIVE_FRAME_COUNT];
    let mut counter_buf1: usize = 0;
    let mut counter_buf2: usize = 0;
    
    
    let mut req: Vec<u8>;
    let mut face_cascade = get_classifier_model("cascades/haarcascade_frontalface_default.xml").expect("unable to get cascade");
    let mut fullbody_cascade = get_classifier_model("cascades/haarcascade_fullbody.xml").expect("unable to get cascade");
    let mut allow_video: bool = true;
    let mut recording_started = false;
    let mut instant_time: Instant = Instant::now();
    let mut video_output: Option<VideoWriter> = None;
    let mut detec1: bool;
    let mut detec2: bool;

    //benchmark vars
    let mut measurement: Instant = Instant::now();
    let interval: Duration = Duration::from_secs(5);
    let mut frame_count: u64 = 0; 
    let mut fps_actual: f64 = 10.0;
    let mut fps_prev: f64 = 10.0;
    
    loop {
        if measurement.elapsed() >= interval {
            fps_prev = fps_actual;
            fps_actual = (frame_count / interval.as_secs()) as f64;
            measurement = Instant::now();
            frame_count = 0;
        } else { 
            frame_count += 1; 
        }

        s.read_exact(&mut seg).await?;
        let content_size: usize = u64::from_be_bytes(seg) as usize;
        seg = [0; 8];
        req = vec![0u8; content_size];

        s.read_exact(&mut req).await?; 

        match imgcodecs::imdecode(&(&req as &[u8]), imgcodecs::IMREAD_COLOR) {
            Ok(mut img) => {

                ReturnMetadata { img , detection: detec1 } = process_person_detection(&mut fullbody_cascade, img).expect("Error detecting step 1.");
                ReturnMetadata { img, detection: detec2 } = process_person_detection(&mut face_cascade, img).expect("Error detecting step 2.");


                //cycle buffers through
                buffer1[counter_buf1] = detec1;
                buffer2[counter_buf2] = detec2;

                if counter_buf1 < FALSE_POSITIVE_FRAME_COUNT - 1 {
                    counter_buf1 += 1;
                } else {
                    counter_buf1 = 0;
                }

                if counter_buf2 < FALSE_POSITIVE_FRAME_COUNT - 1 {
                    counter_buf2 += 1;
                } else {
                    counter_buf2 = 0;
                }

                // start video when buffer signal done
                if (buffer1.iter().all(|&x| x == true) || buffer2.iter().all(|&x| x == true)) 
                && allow_video {
                    if !recording_started {
                        if let Ok(vr) = init_video_writer(
                            &format!("Recordings/Video_Recording_at_{}", 
                            Local::now().to_string()), 
                            img.cols(), 
                            img.rows(),
                            (fps_actual + fps_prev) / 2.0 // fps avg
                        ) {

                            video_output = Some(vr);
                            recording_started = true;
                        }
                    } else {
                        //if recording begins, save frame
                        if let Some(ref mut vr) = video_output {
                            match vr.write(&img) {
                                Ok(()) => {
                                    instant_time = Instant::now();
                                },
                                Err(e) => {
                                    eprintln!("Error writing frame. <{}>", e);
                                }
                            }
                        }
                    }
                } else {
                    
                    if recording_started && (instant_time.elapsed() >= RECORDING_STOP_INTERVAL) {
                        instant_time = Instant::now();
                        //stop saving to file
                        drop(video_output.take());
                        recording_started = false;
                        allow_video = false;
                        buffer1.fill(false);
                        buffer2.fill(false);
                    }
                    else if instant_time.elapsed() >= RECORDING_STOP_INTERVAL && !allow_video {
                        allow_video = true;
                    }
                }


                let mut buf: Vector<u8> = Vector::new();
                imgcodecs::imencode(".jpeg", &mut img, &mut buf, &Vector::<i32>::new()).expect("Error encoding result image.");
                let mut queue = pipe.write().await;
                (*queue) = Some(buf.to_vec());

            },
            Err(e) => {
                eprintln!("Error Converting image: {}", e);
                continue;
            }
        };
    };
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
                sleep(Duration::from_millis(1)).await;
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