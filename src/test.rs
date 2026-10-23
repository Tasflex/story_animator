use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use tokio::process::Command;

#[derive(Deserialize)]
struct ScriptRequest {
    topic: String,
}

#[derive(Serialize)]
struct ScriptResponse {
    script: String,
}

#[derive(Deserialize)]
struct AudioRequest {
    script: String,
    voice_id: String,
}

#[derive(Serialize)]
struct AudioResponse {
    message: String,
    file_path: String,
}

#[derive(Deserialize)]
struct ManimRequest {
    script: String,
    #[serde(default = "default_duration")]
    audio_duration: f64,
}

fn default_duration() -> f64 { 
    30.0 
}

#[derive(Serialize)]
struct ManimResponse {
    message: String,
    file_path: String,
}

#[derive(Deserialize)]
struct RenderRequest {
    script_path: String,
}

#[derive(Serialize)]
struct RenderResponse {
    message: String,
    video_path: String,
}

#[derive(Deserialize)]
struct CombineRequest {
    video_path: String,
    audio_path: String,
}

#[derive(Serialize)]
struct CombineResponse {
    message: String,
    final_video_path: String,
}

#[derive(Deserialize)]
struct MasterRequest {
    topic: String,
    voice_id: String,
}

#[derive(Serialize)]
struct MasterResponse {
    message: String,
    final_video_path: String,
}

#[derive(Clone)]
struct AppState {
    client: Client,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    
    tokio::fs::create_dir_all("storage/audio").await.unwrap();
    tokio::fs::create_dir_all("storage/scripts").await.unwrap();
    tokio::fs::create_dir_all("storage/output").await.unwrap();

    let state = AppState {
        client: Client::new(),
    };

    let app = Router::new()
        .route("/api/script", post(generate_script))
        .route("/api/audio", post(generate_audio))
        .route("/api/manim", post(generate_manim))
        .route("/api/render", post(render_video))
        .route("/api/combine", post(combine_media))
        .route("/api/generate", post(run_pipeline))
        .with_state(state);

    println!("Server running on http://localhost:3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn generate_script(
    State(state): State<AppState>,
    Json(payload): Json<ScriptRequest>,
) -> Json<ScriptResponse> {
    let api_key = env::var("GEMINI_API_KEY").unwrap_or_default();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.5-flash:generateContent?key={}",
        api_key
    );

    let prompt = format!(
        "Write a concise educational narrative about {}. Output only the spoken words, formatted for text-to-speech synthesis without any scene descriptions.",
        payload.topic
    );

    let body = json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }]
    });

    let response = state.client.post(&url).json(&body).send().await.unwrap();

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_default();
        println!("Gemini error: {}", error_body);
        return Json(ScriptResponse { script: format!("ERROR: {}", error_body) });
    }

    let json_data: Value = response.json().await.unwrap();
    let text = json_data["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Json(ScriptResponse { script: text })
}

async fn generate_audio(
    State(state): State<AppState>,
    Json(payload): Json<AudioRequest>,
) -> Json<AudioResponse> {
    let api_key = env::var("ELEVENLABS_API_KEY").unwrap_or_default();
    let url = format!(
        "https://api.elevenlabs.io/v1/text-to-speech/{}",
        payload.voice_id
    );

    let body = json!({
        "text": payload.script,
        "model_id": "eleven_multilingual_v2",
        "voice_settings": {
            "stability": 0.5,
            "similarity_boost": 0.75
        }
    });

    let response = state.client
        .post(&url)
        .header("xi-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();

    if !response.status().is_success() {
        let error_msg = response.text().await.unwrap_or_default();
        println!("ElevenLabs API Error: {}", error_msg);
        return Json(AudioResponse { 
            message: "ElevenLabs API failed".to_string(), 
            file_path: "".to_string() 
        });
    }

    let audio_bytes = response.bytes().await.unwrap();
    let file_path = format!("storage/audio/{}.mp3", uuid::Uuid::new_v4());
    tokio::fs::write(&file_path, &audio_bytes).await.unwrap();

    Json(AudioResponse { 
        message: "Audio generated successfully".to_string(), 
        file_path 
    })
}

async fn generate_manim(
    State(state): State<AppState>,
    Json(payload): Json<ManimRequest>,
) -> Json<ManimResponse> {
    let api_key = env::var("DEEPSEEK_API_KEY").unwrap_or_default();
    let url = "https://api.deepseek.com/chat/completions";

    let prompt = format!(
        "Write a Manim Python script to animate the following narrative. 
        Create a class named StoryAnimation(Scene). 
        CRITICAL RULES:
        1. The total animation duration must be EXACTLY {:.1} seconds to match the voiceover track. You MUST explicitly set `run_time` on your animations and add a final `self.wait(...)` at the very end of your script to fill any remaining time up to {:.1} seconds.
        2. ONLY use standard Manim built-in colors (e.g., RED, BLUE, WHITE, ORANGE, YELLOW) or standard string hex codes (e.g., '#FF4500'). Do NOT invent color constants like ORANGE_RED.
        3. Do NOT use Tex, MathTex, or any LaTeX-based mobjects. Use ONLY Text() for all text, and use standard Unicode characters (like ✓, ★, →) if you need special symbols.
        Narrative: '{}'
        Return ONLY valid Python code.",
        payload.audio_duration, payload.audio_duration, payload.script
    );

    let body = json!({
        "model": "deepseek-chat",
        "messages": [
            {"role": "system", "content": "You are an expert Manim developer. Output exact Python code only."},
            {"role": "user", "content": prompt}
        ]
    });

    let response_result = state.client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    let response = match response_result {
        Ok(res) => res,
        Err(e) => {
            println!("DeepSeek Network Error: {}", e);
            return Json(ManimResponse { message: "Network failure".to_string(), file_path: "".to_string() });
        }
    };

    if !response.status().is_success() {
        let error_msg = response.text().await.unwrap_or_default();
        println!("DeepSeek API Rejected Request: {}", error_msg);
        return Json(ManimResponse { message: "API failed".to_string(), file_path: "".to_string() });
    }

    let json_text = response.text().await.unwrap_or_default();
    let json_data: Value = match serde_json::from_str(&json_text) {
        Ok(val) => val,
        Err(_) => return Json(ManimResponse { message: "Invalid JSON".to_string(), file_path: "".to_string() }),
    };
    
    let content = json_data["choices"][0]["message"]["content"].as_str().unwrap_or("");
    
    // Robustly extract only the Python code, ignoring XML tags or markdown text
    let mut code = content.to_string();
    if let Some(start) = code.find("```python") {
        let rest = &code[start + 9..];
        if let Some(end) = rest.find("```") {
            code = rest[..end].trim().to_string();
        }
    } else if let Some(start) = code.find("```") {
        let rest = &code[start + 3..];
        if let Some(end) = rest.find("```") {
            code = rest[..end].trim().to_string();
        }
    }

    let file_path = format!("storage/scripts/{}.py", uuid::Uuid::new_v4());
    let _ = tokio::fs::write(&file_path, &code).await;

    Json(ManimResponse { message: "Success".to_string(), file_path })
}

async fn render_video(Json(payload): Json<RenderRequest>) -> Json<RenderResponse> {
    let path = std::path::Path::new(&payload.script_path);
    let file_stem = path.file_stem().unwrap().to_str().unwrap();
    let output_file = format!("{}.mp4", file_stem);

    let output = Command::new("venv\\Scripts\\manim.exe")
        .arg(&payload.script_path).arg("StoryAnimation").arg("-ql").arg("-o").arg(&output_file).arg("--media_dir").arg("storage/output")
        .output().await.expect("Failed to execute manim process");

    if output.status.success() {
        Json(RenderResponse {
            message: "Success".to_string(),
            video_path: format!("storage/output/videos/{}/480p15/{}", file_stem, output_file),
        })
    } else {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        println!("Manim Error: {}", error_msg);
        Json(RenderResponse { message: "Render failed".to_string(), video_path: "".to_string() })
    }
}

async fn combine_media(Json(payload): Json<CombineRequest>) -> Json<CombineResponse> {
    let final_path = format!("storage/output/final_{}.mp4", uuid::Uuid::new_v4());
    
    // Uses the tpad filter to clone the final video frame to match the audio length perfectly
    let output = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i").arg(&payload.video_path)
        .arg("-i").arg(&payload.audio_path)
        .arg("-filter_complex").arg("[0:v]tpad=stop_mode=clone:stop_duration=999[v]")
        .arg("-map").arg("[v]")
        .arg("-map").arg("1:a:0")
        .arg("-c:v").arg("libx264")
        .arg("-c:a").arg("aac")
        .arg("-shortest")
        .arg(&final_path)
        .output().await.expect("Failed to execute ffmpeg process");

    if output.status.success() {
        Json(CombineResponse { message: "Success".to_string(), final_video_path: final_path })
    } else {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        println!("FFmpeg Error: {}", error_msg);
        Json(CombineResponse { message: "Combine failed".to_string(), final_video_path: "".to_string() })
    }
}

async fn run_pipeline(
    State(state): State<AppState>,
    Json(payload): Json<MasterRequest>,
) -> Result<Json<MasterResponse>, (StatusCode, String)> {
    let base_url = "http://localhost:3000";
    let err = |msg: String| (StatusCode::INTERNAL_SERVER_ERROR, msg);

    println!("Step 1: Generating Script...");
    let script_res: Value = state.client.post(format!("{}/api/script", base_url))
        .json(&json!({ "topic": payload.topic }))
        .send().await.map_err(|e| err(format!("script network: {e}")))?
        .json().await.map_err(|e| err(format!("script decode: {e}")))?;
        
    let script = script_res["script"].as_str()
        .ok_or_else(|| err(format!("script missing: {script_res}")))?
        .to_string();

    if script.starts_with("ERROR:") {
        return Err(err(script));
    }

    println!("Step 2: Synthesizing Voice...");
    let audio_res: Value = state.client.post(format!("{}/api/audio", base_url))
        .json(&json!({ "script": script, "voice_id": payload.voice_id }))
        .send().await.map_err(|e| err(format!("audio network: {e}")))?
        .json().await.map_err(|e| err(format!("audio decode: {e}")))?;
        
    let audio_path = audio_res["file_path"].as_str()
        .ok_or_else(|| err(format!("audio file_path missing: {audio_res}")))?
        .to_string();

    if audio_path.is_empty() {
        return Err(err("Pipeline aborted: Audio generation failed.".to_string()));
    }

    let duration = mp3_duration::from_path(&audio_path)
        .map_err(|e| err(format!("Failed to read MP3 duration: {e}")))?
        .as_secs_f64();
    println!("Calculated Voice Duration: {:.1} seconds", duration);

    println!("Step 3: Writing Manim Code...");
    let manim_res: Value = state.client.post(format!("{}/api/manim", base_url))
        .json(&json!({ "script": script, "audio_duration": duration }))
        .send().await.map_err(|e| err(format!("manim network: {e}")))?
        .json().await.map_err(|e| err(format!("manim decode: {e}")))?;
        
    let script_path = manim_res["file_path"].as_str()
        .ok_or_else(|| err(format!("manim file_path missing: {manim_res}")))?
        .to_string();

    if script_path.is_empty() {
        return Err(err("Pipeline aborted: Code generation failed.".to_string()));
    }

    println!("Step 4: Rendering Video...");
    let render_res: Value = state.client.post(format!("{}/api/render", base_url))
        .json(&json!({ "script_path": script_path }))
        .send().await.map_err(|e| err(format!("render network: {e}")))?
        .json().await.map_err(|e| err(format!("render decode: {e}")))?;
        
    let video_path = render_res["video_path"].as_str()
        .ok_or_else(|| err(format!("video_path missing: {render_res}")))?
        .to_string();

    if video_path.is_empty() {
        return Err(err("Pipeline aborted: Manim rendering failed.".to_string()));
    }

    println!("Step 5: Multiplexing Audio and Video...");
    let combine_res: Value = state.client.post(format!("{}/api/combine", base_url))
        .json(&json!({ "video_path": video_path, "audio_path": audio_path }))
        .send().await.map_err(|e| err(format!("combine network: {e}")))?
        .json().await.map_err(|e| err(format!("combine decode: {e}")))?;
        
    let final_path = combine_res["final_video_path"].as_str()
        .ok_or_else(|| err(format!("final_video_path missing: {combine_res}")))?
        .to_string();

    if final_path.is_empty() {
        return Err(err("Pipeline aborted: FFmpeg multiplexing failed.".to_string()));
    }

    println!("Pipeline Complete! Saved to: {}", final_path);

    Ok(Json(MasterResponse {
        message: "Full pipeline executed successfully".to_string(),
        final_video_path: final_path,
    }))
}