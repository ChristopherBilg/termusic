pub mod encrypt;
mod kugou;
/**
 * MIT License
 *
 * termusic - Copyright (c) 2021 Larry Hao
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
pub mod lrc;
mod migu;
mod netease;
use crate::ui::activity::{main::TransferState, tageditor::SearchLyricState};
use anyhow::{anyhow, bail, Result};
use id3::{
    frame::{Lyrics, Picture, PictureType},
    Tag, Version,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, sleep};
use std::time::Duration;
use ytd_rs::{Arg, ResultType, YoutubeDL};

#[derive(Deserialize, Serialize)]
pub struct SongTag {
    artist: Option<String>,
    title: Option<String>,
    album: Option<String>,
    lang_ext: Option<String>,
    service_provider: Option<ServiceProvider>,
    song_id: Option<String>,
    lyric_id: Option<String>,
    url: Option<String>,
    pic_id: Option<String>,
    album_id: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub enum ServiceProvider {
    Netease,
    Kugou,
    Migu,
}

impl std::fmt::Display for ServiceProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let service_provider = match self {
            Self::Netease => "Netease",
            Self::Kugou => "Kugou",
            Self::Migu => "Migu",
        };
        write!(f, "{}", service_provider)
    }
}

// Search function of 3 servers. Run in parallel to get results faster.
pub fn search(search_str: &str, tx_tageditor: Sender<SearchLyricState>) {
    let mut results: Vec<SongTag> = Vec::new();
    let (tx, rx): (Sender<Vec<SongTag>>, Receiver<Vec<SongTag>>) = mpsc::channel();

    let tx1 = tx.clone();
    let search_str_netease = search_str.to_string();
    let handle_netease = thread::spawn(move || -> Result<()> {
        let mut netease_api = netease::Api::new();
        if let Ok(results) = netease_api.search(&search_str_netease, 1, 0, 30) {
            let result_new: Vec<SongTag> = serde_json::from_str(&results)?;
            if tx1.send(result_new).is_ok() {}
        }
        Ok(())
    });

    let tx2 = tx.clone();
    let search_str_migu = search_str.to_string();
    let handle_migu = thread::spawn(move || -> Result<()> {
        let migu_api = migu::Api::new();
        if let Ok(results) = migu_api.search(&search_str_migu, 1, 0, 30) {
            let result_new: Vec<SongTag> = serde_json::from_str(&results)?;
            if tx2.send(result_new).is_ok() {}
        }
        Ok(())
    });

    let kugou_api = kugou::Api::new();
    let search_str_kugou = search_str.to_string();
    let handle_kugou = thread::spawn(move || -> Result<()> {
        if let Ok(r) = kugou_api.search(&search_str_kugou, 1, 0, 30) {
            let result_new: Vec<SongTag> = serde_json::from_str(&r)?;
            if tx.send(result_new).is_ok() {}
        }
        Ok(())
    });

    thread::spawn(move || {
        if handle_netease.join().is_ok() {
            if let Ok(result_new) = rx.try_recv() {
                results.extend(result_new);
            }
        }

        if handle_migu.join().is_ok() {
            if let Ok(result_new) = rx.try_recv() {
                results.extend(result_new);
            }
        }

        if handle_kugou.join().is_ok() {
            if let Ok(result_new) = rx.try_recv() {
                results.extend(result_new);
            }
        }

        let _drop = tx_tageditor.send(SearchLyricState::Finish(results));
    });
}

impl SongTag {
    pub fn artist(&self) -> Option<&str> {
        match self.artist.as_ref() {
            Some(artist) => Some(artist),
            None => None,
        }
    }

    pub fn album(&self) -> Option<&str> {
        match self.album.as_ref() {
            Some(album) => Some(album),
            None => None,
        }
    }
    /// Optionally return the title of the song
    /// If `None` it wasn't able to read the tags
    pub fn title(&self) -> Option<&str> {
        match self.title.as_ref() {
            Some(title) => Some(title),
            None => None,
        }
    }

    pub fn lang_ext(&self) -> Option<&str> {
        match self.lang_ext.as_ref() {
            Some(lang_ext) => Some(lang_ext),
            None => None,
        }
    }

    pub const fn service_provider(&self) -> Option<&ServiceProvider> {
        match self.service_provider.as_ref() {
            Some(service_provider) => Some(service_provider),
            None => None,
        }
    }

    pub fn url(&self) -> Option<String> {
        self.url.as_ref().map(std::string::ToString::to_string)
    }
    // get lyric by lyric_id
    pub fn fetch_lyric(&self) -> Result<String> {
        let mut lyric_string = String::new();

        match self.service_provider {
            Some(ServiceProvider::Kugou) => {
                let kugou_api = kugou::Api::new();
                if let Some(lyric_id) = &self.lyric_id {
                    lyric_string = kugou_api.song_lyric(lyric_id)?;
                }
            }
            Some(ServiceProvider::Netease) => {
                let mut netease_api = netease::Api::new();
                if let Some(lyric_id) = &self.lyric_id {
                    lyric_string = netease_api.song_lyric(lyric_id)?;
                }
            }
            Some(ServiceProvider::Migu) => {
                let migu_api = migu::Api::new();
                if let Some(lyric_id) = &self.lyric_id {
                    lyric_string = migu_api.song_lyric(lyric_id)?;
                }
            }
            None => {}
        }

        Ok(lyric_string)
    }

    // get photo by pic_id(kugou/netease) or song_id(migu)
    pub fn fetch_photo(&self) -> Result<Picture> {
        let mut encoded_image_bytes: Vec<u8> = Vec::new();

        match self.service_provider {
            Some(ServiceProvider::Kugou) => {
                let kugou_api = kugou::Api::new();
                if let Some(p) = &self.pic_id {
                    if let Some(album_id) = &self.album_id {
                        encoded_image_bytes = kugou_api.pic(p, album_id)?;
                    }
                }
            }
            Some(ServiceProvider::Netease) => {
                let mut netease_api = netease::Api::new();
                if let Some(p) = &self.pic_id {
                    encoded_image_bytes = netease_api.pic(p)?;
                }
            }
            Some(ServiceProvider::Migu) => {
                let migu_api = migu::Api::new();
                if let Some(p) = &self.song_id {
                    encoded_image_bytes = migu_api.pic(p)?;
                }
            }
            None => {}
        }

        if encoded_image_bytes.is_empty() {
            bail!("failed to fetch image");
        }

        Ok(Picture {
            mime_type: "image/jpeg".to_string(),
            picture_type: PictureType::Other,
            description: "some image".to_string(),
            data: encoded_image_bytes,
        })
    }

    pub fn download(&self, file: &str, tx_tageditor: Sender<TransferState>) -> Result<()> {
        let p: &Path = Path::new(file);
        let p_parent = PathBuf::from(p.parent().unwrap_or_else(|| Path::new("/tmp")));
        let song_id = self
            .song_id
            .as_ref()
            .ok_or_else(|| anyhow!("error downloading because no song id is found"))?;
        let artist = self
            .artist
            .clone()
            .unwrap_or_else(|| "Unknown Artist".to_string());
        let title = self
            .title
            .clone()
            .unwrap_or_else(|| "Unknown Title".to_string());

        let album = self.album.clone().unwrap_or_else(|| String::from("N/A"));
        let lyric = self.fetch_lyric();
        let photo = self.fetch_photo();
        let album_id = self.album_id.clone().unwrap_or_else(|| String::from("N/A"));

        let filename = format!("{}-{}.%(ext)s", artist, title);

        let args = vec![
            Arg::new("--quiet"),
            Arg::new_with_arg("--output", filename.as_ref()),
            Arg::new("--extract-audio"),
            Arg::new_with_arg("--audio-format", "mp3"),
        ];

        let p_full = format!(
            "{}/{}-{}.mp3",
            p_parent.to_str().unwrap_or("/tmp"),
            artist,
            title
        );
        if std::fs::remove_file(Path::new(p_full.as_str())).is_err() {}

        let mp3_url = self.url.clone().unwrap_or_else(|| String::from("N/A"));
        if mp3_url.starts_with("Copyright") {
            bail!("Copyright protected, please select another item.");
        }
        let mut url = mp3_url;

        if let Some(s) = &self.service_provider {
            match s {
                ServiceProvider::Netease => {
                    let mut netease_api = netease::Api::new();
                    url = netease_api.song_url(song_id)?;
                }
                ServiceProvider::Migu => {}
                ServiceProvider::Kugou => {
                    let kugou_api = kugou::Api::new();
                    url = kugou_api.song_url(song_id, &album_id)?;
                }
            }
        }

        if url.is_empty() {
            bail!("url fetch failed, please try another item.");
        }

        let ytd = YoutubeDL::new(&p_parent, args, &url)?;

        let tx = tx_tageditor;
        thread::spawn(move || {
            let _drop = tx.send(TransferState::Running);
            // start download
            let download = ytd.download();

            // check what the result is and print out the path to the download or the error
            match download.result_type() {
                ResultType::SUCCESS => {
                    let mut song_id3tag = Tag::new();
                    song_id3tag.set_album(album);
                    song_id3tag.set_title(title);
                    song_id3tag.set_artist(artist);
                    if let Ok(l) = lyric {
                        song_id3tag.add_lyrics(Lyrics {
                            lang: String::from("chi"),
                            description: String::from("saved by termusic."),
                            text: l,
                        });
                    }

                    if let Ok(p) = photo {
                        song_id3tag.add_picture(p);
                    }

                    let file = p_full.as_str();
                    if song_id3tag.write_to_path(file, Version::Id3v24).is_ok() {
                        let _drop = tx.send(TransferState::Success);
                        sleep(Duration::from_secs(5));
                        let _drop = tx.send(TransferState::Completed(Some(p_full)));
                    } else {
                        let _drop = tx.send(TransferState::ErrEmbedData);
                        sleep(Duration::from_secs(5));
                        let _drop = tx.send(TransferState::Completed(None));
                    }
                }
                ResultType::IOERROR | ResultType::FAILURE => {
                    let _drop = tx.send(TransferState::ErrDownload);
                    sleep(Duration::from_secs(5));
                    let _drop = tx.send(TransferState::Completed(None));
                }
            };
        });
        Ok(())
    }
}
