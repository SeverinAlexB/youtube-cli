use crate::model::{SearchResult, TranscriptResult, VideoDetail};
use std::time::SystemTime;

pub fn format_search_results(result: &SearchResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "## YouTube: \"{}\" (showing {})\n\n",
        result.query,
        result.videos.len()
    ));

    for (i, video) in result.videos.iter().enumerate() {
        out.push_str(&format!("{}. **{}**\n", i + 1, video.title));

        let mut meta_parts: Vec<String> = Vec::new();
        meta_parts.push(video.channel.clone());
        if let Some(ref dur) = video.duration {
            meta_parts.push(dur.clone());
        }
        if let Some(ref views) = video.views {
            meta_parts.push(views.clone());
        }
        if let Some(ref published) = video.published {
            meta_parts.push(published.clone());
        }

        out.push_str(&format!("   {} \n", meta_parts.join(" — ")));
        out.push_str(&format!(
            "   https://youtube.com/watch?v={}\n",
            video.video_id
        ));

        if let Some(ref desc) = video.description {
            let truncated = if desc.len() > 150 {
                format!("{}...", &desc[..150])
            } else {
                desc.clone()
            };
            out.push_str(&format!("   {}\n", truncated));
        }

        out.push('\n');
    }

    out
}

pub fn format_transcript(result: &TranscriptResult, timestamps: bool) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!("## Transcript: {}\n", result.title));
    out.push_str(&format!(
        "Channel: {} — Language: {}{}  \n",
        result.channel,
        result.language,
        if result.is_auto_generated {
            " (auto-generated)"
        } else {
            ""
        }
    ));
    out.push_str(&format!(
        "https://youtube.com/watch?v={}\n\n",
        result.video_id
    ));

    if timestamps {
        for entry in &result.entries {
            let ts = format_timestamp(entry.start);
            out.push_str(&format!("[{}] {}\n", ts, entry.text));
        }
    } else {
        // Continuous text mode — join entries with spaces, wrap at ~80 chars
        let mut line = String::new();
        for entry in &result.entries {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&entry.text);
            if line.len() >= 80 {
                out.push_str(&line);
                out.push('\n');
                line.clear();
            }
        }
        if !line.is_empty() {
            out.push_str(&line);
            out.push('\n');
        }
    }

    out
}

pub fn format_video_detail(detail: &VideoDetail) -> String {
    let mut out = String::new();

    out.push_str(&format!("# {}\n\n", detail.title));

    out.push_str("## Overview\n\n");
    out.push_str(&format!("- **Channel**: {}\n", detail.channel));
    if let Some(ref views) = detail.view_count {
        out.push_str(&format!("- **Views**: {}\n", format_number(views)));
    }
    if let Some(secs) = detail.length_seconds {
        out.push_str(&format!("- **Duration**: {}\n", format_duration(secs)));
    }
    if detail.is_live {
        out.push_str("- **Live**: Yes\n");
    }
    out.push_str(&format!(
        "- **URL**: https://youtube.com/watch?v={}\n",
        detail.video_id
    ));

    if !detail.caption_languages.is_empty() {
        out.push_str("\n## Available Captions\n\n");
        for lang in &detail.caption_languages {
            out.push_str(&format!(
                "- {} ({}){}\n",
                lang.language_name,
                lang.language_code,
                if lang.is_auto_generated {
                    " [auto-generated]"
                } else {
                    ""
                }
            ));
        }
    } else {
        out.push_str("\n*No captions available*\n");
    }

    if let Some(ref desc) = detail.description {
        if !desc.is_empty() {
            out.push_str("\n## Description\n\n");
            out.push_str(desc);
            out.push('\n');
        }
    }

    if !detail.keywords.is_empty() {
        out.push_str("\n## Keywords\n\n");
        out.push_str(&detail.keywords.join(", "));
        out.push('\n');
    }

    out
}

pub fn format_timestamp(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

pub fn format_duration(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let mins = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{}:{:02}", mins, secs)
    }
}

pub fn format_number(s: &str) -> String {
    // Input is a raw number string like "1234567"
    // Output: "1,234,567"
    let chars: Vec<char> = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if chars.is_empty() {
        return s.to_string();
    }
    let mut result = String::new();
    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*ch);
    }
    result
}

pub fn format_cached_at(time: SystemTime) -> String {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Simple date formatting without external crate
    let days_since_epoch = secs / 86400;
    let mut year = 1970u64;
    let mut remaining_days = days_since_epoch;

    loop {
        let days_in_year =
            if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
                366
            } else {
                365
            };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u64; 12] =
        if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };

    let mut month = 0;
    for (i, &days) in days_in_months.iter().enumerate() {
        if remaining_days < days {
            month = i + 1;
            break;
        }
        remaining_days -= days;
    }
    if month == 0 {
        month = 12;
    }
    let day = remaining_days + 1;

    format!("{}-{:02}-{:02}", year, month, day)
}
