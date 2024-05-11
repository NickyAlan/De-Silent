extern crate hound;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::{collections::HashMap, vec};
use hound::{WavReader, SampleFormat, WavWriter, WavSpec};

fn sec2rate(sec: f32, sample_rate: i32) -> i32 {
    //! convert second to rate
    (sec*(sample_rate as f32)*2.0) as i32
}

fn rate2sec(rate: i32, sample_rate: i32) -> f32 {
    //!convert rate to second
    (rate as f32 /(sample_rate*2) as f32) as f32
}

fn percentile(mut arr: Vec<f32>, q: i32) -> f32 {
    //! find value of q percentile in array
    let n = arr.len() as f32;
    let idx = (q as f32)*(n+1.0)/100.0;
    arr.sort_by(|a, b| a.partial_cmp(b).unwrap());
    arr[idx as usize]
}

fn get_under_threshold(arr: Vec<f32>, threshold: f32, cut_duration_sec: f32, sample_rate: i32, add_smooth_sec: f32) -> Vec<Vec<usize>> {
    //! alpha: percentile for cut point if sound silent than threshold_alpha it'll cut off.
    //! cut_duration_sec: if sound is silent longer than this parameter it'll cut off.
    let cut_duration_rate = sec2rate(cut_duration_sec, sample_rate);
    let mut under_ts_rate = 0;
    let mut under_ts: HashMap<usize, usize> = HashMap::new();
    let mut is_start_rate = true;
    let mut start_cut = 0;
    let add_smooth_rate = sec2rate(add_smooth_sec, sample_rate) as usize;
    for (rate_count, val) in arr.iter().enumerate() {
        if val < &threshold {
            under_ts_rate += 1;
            if is_start_rate {
                start_cut = rate_count;
                is_start_rate = false;
            }
            if under_ts_rate >= cut_duration_rate {
                under_ts.insert(start_cut + add_smooth_rate, rate_count - add_smooth_rate);
            }
        } else {
            // reset every thing
            is_start_rate = true;
            under_ts_rate = 0;
        }
    } 

    let mut under_ts_vec: Vec<Vec<usize>> = vec![];
    for (start_rate, stop_rate) in under_ts.iter() {
        under_ts_vec.push(vec![*start_rate, *stop_rate])
    }

    // sort order because hashmap is random order
    under_ts_vec.sort_by_key(|x| (x[0]));
    under_ts_vec
}

fn get_keep_rate(under_ts_vec: Vec<Vec<usize>>, last_rate: usize) -> Vec<Vec<usize>> {
    //![[500, 1000], [2000, 4000]]-> [[0, 500], [1000, 2000], [4000, last_rate]]
    let mut keeps_rate: Vec<Vec<usize>> = vec![];
    let mut prev_rate = 0;
    for rates in &under_ts_vec {
        keeps_rate.push(vec![prev_rate, rates[0]]);
        prev_rate = rates[1];
    }
    
    // forgot last rate
    let last_under_ts = under_ts_vec[under_ts_vec.len() - 1][1];
    if last_under_ts != last_rate {
        keeps_rate.push(vec![last_under_ts, last_rate]);
    }

    keeps_rate
}

fn crete_temp_dir() {
    _ = fs::create_dir("temp");
}

fn concat_videos(save_path: &str) -> Result<(), std::io::Error> {
    let dir_path: Result<Vec<_>, _> = fs::read_dir("temp")?.collect();
    
    // write .txt for path list
    let mut file = File::create("temp.txt")?;
    for file_name in 0..dir_path?.len() {
        writeln!(file, "file 'temp/temp_{}.mp4'", file_name)?;
    }

    // combined files
    let output = Command::new("ffmpeg")
                                .args(["-f", "concat", "-safe", "0", "-i", &format!("temp.txt"), "-c", "copy", save_path])
                                .output()
                                .expect("failed to execute process");

    output.stdout;    
    Ok(())
}

fn main() {
    let mut file_path = "name.wav";
    let save_path = format!("{} - res.wav", file_path);
    let q = 85;
    let cut_duration = 0.4;
    let add_smooth_sec = 0.15;
    
    println!("\nDe-silent: {}", file_path);
    let file_name: &str = file_path.split(".")
                        .next()
                        .expect("No filename found");
    let file_video_path = &format!("{}.mp4", file_name);

    if file_path.contains("mp4") {
        let _ = Command::new("ffmpeg")
        .args(["-i", file_path, "-ac", "2", "-f", "wav", "temp.wav"])
        .output()
        .expect("failed to execute process");
        // change for using temporary wav file
        file_path = "temp.wav";
    }

    let reader = WavReader::open(file_path).unwrap();
    let info = reader.spec();
    let num_channels = info.channels as usize;
    // sample_rate = 0.5 sec
    let sample_rate = info.sample_rate as usize;
    let sample_format = info.sample_format;
    let bit_depth = info.bits_per_sample;

    // into array
    match sample_format {
        SampleFormat::Float => {
            let samples: Vec<f32> = reader.into_samples::<f32>().map(Result::unwrap).collect();  
            let mut above_zero_wave: Vec<f32> = vec![];
            let mut no_zero_wave: Vec<f32> = vec![];
            for val in &samples {
                if val > &0.0 {
                    above_zero_wave.push(*val);
                    no_zero_wave.push(*val);
                } else {
                    above_zero_wave.push(0.0);
                }
            };

            let threshold = percentile(no_zero_wave, q);
            let under_ts_vec = get_under_threshold(above_zero_wave.clone(), threshold, cut_duration, sample_rate as i32, add_smooth_sec);
            let keeps_rate = get_keep_rate(under_ts_vec.clone(), above_zero_wave.len() - 1);
            let mut wave_res: Vec<f32> = vec![];
            for rates in keeps_rate {
                wave_res.extend(&samples[rates[0]..rates[1]])
            }

            // write .wav file
            let spec = WavSpec {
                sample_rate: sample_rate as u32,
                channels: num_channels as u16,
                bits_per_sample: bit_depth as u16,
                sample_format: hound::SampleFormat::Float,
            };
            
            let mut writer = WavWriter::create(save_path, spec).unwrap();
            for sample in wave_res {
                writer.write_sample(sample).unwrap();
            }
        }
        SampleFormat::Int => {
            let samples: Vec<i32> = reader.into_samples::<i32>().map(Result::unwrap).collect();  
            let mut above_zero_wave: Vec<f32> = vec![];
            let mut no_zero_wave: Vec<f32> = vec![];
            for val in &samples {
                if val > &0 {
                    above_zero_wave.push(*val as f32);
                    no_zero_wave.push(*val as f32);
                } else {
                    above_zero_wave.push(0.0);
                }
            };

            let threshold = percentile(no_zero_wave, q);
            let under_ts_vec = get_under_threshold(above_zero_wave.clone(), threshold, cut_duration, sample_rate as i32, add_smooth_sec);
            let keeps_rate = get_keep_rate(under_ts_vec.clone(), above_zero_wave.len() - 1);

            // mp4 save
            if save_path.contains("mp4") {
                crete_temp_dir();
                println!(" -- keep seconds -- ");
                for (idx, rates)  in keeps_rate.iter().enumerate() {
                    let start_sec = rate2sec(rates[0] as i32, sample_rate as i32);
                    let stop_sec = rate2sec(rates[1] as i32, sample_rate as i32);

                    println!("#{}: {} -> {}", idx+1, start_sec, stop_sec);

                    let _ = Command::new("ffmpeg")
                        .args(["-i", file_video_path, "-ss", &format!("{}", start_sec), "-t",  &format!("{}", stop_sec - start_sec), &format!("temp/temp_{}.mp4", idx)])
                        .output()
                        .expect("failed to execute process");
                }

                // concat and delete temp videos
                concat_videos(&save_path).unwrap();
                fs::remove_file("temp.txt").unwrap();
                fs::remove_dir_all("temp").unwrap();

                println!("Complete: {}", save_path);

            } else {
                let mut wave_res: Vec<i32> = vec![];
                for rates in keeps_rate {
                    wave_res.extend(&samples[rates[0]..rates[1]])
                }
                
                // write .wav file
                let spec = WavSpec {
                    sample_rate: sample_rate as u32,
                    channels: num_channels as u16,
                    bits_per_sample: bit_depth as u16,
                    sample_format: hound::SampleFormat::Int,
                };
                
                let mut writer = WavWriter::create(save_path, spec).unwrap();
                for sample in wave_res {
                    writer.write_sample(sample).unwrap();
                }
            }

            // remove temp.wav
            fs::remove_file("temp.wav").unwrap();
        }
    }
}
