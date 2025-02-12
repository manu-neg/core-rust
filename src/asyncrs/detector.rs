use opencv::core::{Rect, Scalar, Size, Vector};
use opencv::prelude::*;
use opencv::{objdetect, imgproc};

pub struct ReturnMetadata {
    pub img: Mat,
    pub detection: bool
}

type Result<T> = opencv::Result<T>;
const SCALE_FACTOR: f64 = 0.5;
const INVERSE_SCALE_FACTOR: i32 = (1f64 / SCALE_FACTOR) as i32;


fn to_grayscale(img: &Mat) -> Result<Mat> {
    let mut buffer = Mat::default();
    imgproc::cvt_color(img, &mut buffer, imgproc::COLOR_BGR2GRAY, 0)?;
    return Ok(buffer);
}

fn resize_with_scale(img: &Mat) -> Result<Mat> {
    const AUTOMATIC_SIZE: Size = Size {
        width: 0,
        height: 0
    };
    let mut result: Mat = Mat::default();
    imgproc::resize(img,
         &mut result,
         AUTOMATIC_SIZE,
         SCALE_FACTOR,
         SCALE_FACTOR,
         imgproc::INTER_LINEAR
        )?;
    return Ok(result);

}

fn histogram_equalization(img: &Mat) -> Result<Mat> {
    let mut result: Mat = Mat::default();
    imgproc::equalize_hist(img, &mut result)?;
    return Ok(result);
}


fn processing(img: &Mat) -> Result<Mat> {
    let grayscale = to_grayscale(img)?;
    let reduced = resize_with_scale(&grayscale)?;
    let equalized = histogram_equalization(&reduced)?;
    return Ok(equalized);

}


fn cascade_detector(classifier: &mut objdetect::CascadeClassifier, img: &Mat) -> Result<Vector<Rect>> {
    const SCALE_FACTOR: f64 = 1.1;
    const MIN_NEIGHBORS: i32 = 2;
    const FLAGS: i32 = 0;
    const MIN_SIZE: Size = Size {
        width: 30,
        height: 30,
    };
    const MAX_SIZE: Size = Size {
        width: 0,
        height: 0,
    };

    let mut faces: Vector<opencv::core::Rect> = Vector::new();
    classifier.detect_multi_scale(
        &img,
        &mut faces,
        SCALE_FACTOR,
        MIN_NEIGHBORS,
        FLAGS,
        MIN_SIZE,
        MAX_SIZE,
    )?;
    Ok(faces)

}

fn trace_img(img: &mut Mat, trace: Rect) -> Result<()> {
    let scaled_item = Rect {
        x: trace.x * INVERSE_SCALE_FACTOR,
        y: trace.y * INVERSE_SCALE_FACTOR,
        width: trace.width * INVERSE_SCALE_FACTOR,
        height: trace.height * INVERSE_SCALE_FACTOR
    };
    const THICKNESS: i32 = 4;
    const LINE_TYPE: i32 = 8;
    const SHIFT: i32 = 0;
    let color = Scalar::new(0f64, 0f64, 255f64, -1f64);

    imgproc::rectangle(img, scaled_item, color, THICKNESS, LINE_TYPE, SHIFT)?;
    return Ok(());

}

pub fn get_classifier_model(dir: &str) -> Result<objdetect::CascadeClassifier> {
    return objdetect::CascadeClassifier::new(dir);

}

pub fn process_person_detection(classifier: &mut objdetect::CascadeClassifier, mut img: Mat) -> Result<ReturnMetadata> {
    let processed = processing(&img)?; 
    let objects= cascade_detector(classifier, &processed)?;
    let detected = objects.len() > 0;
    for item in objects {
        trace_img(&mut img, item)?;
    }
    return Ok(ReturnMetadata { img: img, detection: detected});
}