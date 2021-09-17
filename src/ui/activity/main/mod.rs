//! ## `MainActivity`
//!
//! `main_activity` is the module which implements the Main activity, which is the activity to
//! work on termusic app

mod playlist;
mod queue;
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
// Submodules
// mod actions;
// mod config;
mod update;
mod view;
mod youtube_options;

// Locals
use super::{Activity, Context, ExitReason, Status};
use crate::app::PlayerCommand;
use crate::{
    config::{Termusic, MUSIC_DIR},
    // player::{Generic, Player},
    song::Song,
    ui::activity::tageditor::TagEditorActivity,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use log::error;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::sleep;
use std::time::Duration;
use tui_realm_treeview::Tree;
use tuirealm::{Payload, Value, View};
use youtube_options::{YoutubeOptions, YoutubeSearchState};

// -- components
const COMPONENT_LABEL_HELP: &str = "LABEL_HELP";
const COMPONENT_PARAGRAPH_LYRIC: &str = "PARAGRAPH_LYRIC";
const COMPONENT_TABLE_QUEUE: &str = "SCROLLTABLE_QUEUE";
const COMPONENT_TABLE_YOUTUBE: &str = "SCROLLTABLE_YOUTUBE";
const COMPONENT_TREEVIEW: &str = "TREEVIEW";
const COMPONENT_PROGRESS: &str = "PROGRESS";
const COMPONENT_TEXT_HELP: &str = "TEXT_HELP";
const COMPONENT_INPUT_URL: &str = "INPUT_URL";
const COMPONENT_TEXT_ERROR: &str = "TEXT_ERROR";
const COMPONENT_CONFIRMATION_RADIO: &str = "CONFIRMATION_RADIO";
const COMPONENT_CONFIRMATION_INPUT: &str = "CONFIRMATION_INPUT";
const COMPONENT_TEXT_MESSAGE: &str = "TEXT_MESSAGE";

/// ### `ViewLayout`
///

/// ## `MainActivity`
///
/// Main activity states holder
pub struct TermusicActivity {
    exit_reason: Option<ExitReason>,
    context: Option<Context>, // Context holder
    view: View,               // View
    redraw: bool,
    path: PathBuf,
    tree: Tree,
    // player: Player,
    queue_items: VecDeque<Song>,
    time_pos: u64,
    status: Option<Status>,
    current_song: Option<Song>,
    sender: Sender<TransferState>,
    receiver: Receiver<TransferState>,
    yanked_node_id: Option<String>,
    config: Termusic,
    youtube_options: YoutubeOptions,
    sender_message: Sender<MessageState>,
    receiver_message: Receiver<MessageState>,
    sender_youtubesearch: Sender<YoutubeSearchState>,
    receiver_youtubesearch: Receiver<YoutubeSearchState>,
    sender_queueitems: Sender<VecDeque<Song>>,
    receiver_queueitems: Receiver<VecDeque<Song>>,
    sender_player_command: Sender<PlayerCommand>,
    sender_progress: Sender<u64>,
    receiver_progress: Receiver<u64>,
}

pub enum MessageState {
    Show((String, String)),
    Hide,
}

// TransferState is used to describe the status of download
pub enum TransferState {
    Running, // indicates progress
    Success,
    Completed(Option<String>),
    ErrDownload,
    ErrEmbedData,
}

// StatusLine shows the status of download
#[derive(Copy, Clone)]
pub enum StatusLine {
    Default,
    Success,
    Running,
    Error,
}

impl Default for TermusicActivity {
    fn default() -> Self {
        // Initialize user input
        let mut user_input_buffer: Vec<String> = Vec::with_capacity(16);
        for _ in 0..16 {
            user_input_buffer.push(String::new());
        }

        let full_path = shellexpand::tilde(MUSIC_DIR);
        let p: &Path = Path::new(full_path.as_ref());
        let (tx, rx): (Sender<TransferState>, Receiver<TransferState>) = mpsc::channel();
        let (tx2, rx2): (Sender<MessageState>, Receiver<MessageState>) = mpsc::channel();
        let (tx3, rx3): (Sender<YoutubeSearchState>, Receiver<YoutubeSearchState>) =
            mpsc::channel();
        let (tx4, rx4): (Sender<VecDeque<Song>>, Receiver<VecDeque<Song>>) = mpsc::channel();
        let (tx5, _): (Sender<PlayerCommand>, Receiver<PlayerCommand>) = mpsc::channel();
        let (tx6, rx6): (Sender<u64>, Receiver<u64>) = mpsc::channel();
        Self {
            exit_reason: None,
            context: None,
            view: View::init(),
            redraw: true, // Draw at first `on_draw`
            tree: Tree::new(Self::dir_tree(p, 3)),
            path: p.to_path_buf(),
            queue_items: VecDeque::with_capacity(100),
            time_pos: 0,
            status: None,
            current_song: None,
            sender: tx,
            receiver: rx,
            yanked_node_id: None,
            config: Termusic::default(),
            youtube_options: YoutubeOptions::new(),
            sender_message: tx2,
            receiver_message: rx2,
            sender_youtubesearch: tx3,
            receiver_youtubesearch: rx3,
            sender_queueitems: tx4,
            receiver_queueitems: rx4,
            sender_player_command: tx5,
            sender_progress: tx6,
            receiver_progress: rx6,
        }
    }
}

impl TermusicActivity {
    pub fn init_config(&mut self, config: &Termusic, tx: Sender<PlayerCommand>) {
        self.config = config.clone();
        let music_dir = self.config.music_dir.clone();
        let full_path = shellexpand::tilde(&music_dir);
        let p: &Path = Path::new(full_path.as_ref());
        self.scan_dir(p);
        self.sender_player_command = tx;
    }
    pub fn run(&mut self) {
        match self.status {
            Some(Status::Stopped) => {
                // self.update_queue_items();
                if self.queue_items.is_empty() {
                    return;
                }
                self.status = Some(Status::Running);
                if let Some(song) = self.queue_items.pop_front() {
                    if let Some(file) = song.file() {
                        // self.player.queue_and_play(file);
                        self.sender_player_command
                            .send(PlayerCommand::Play(file.to_string()))
                            .ok();
                    }
                    self.queue_items.push_back(song.clone());
                    self.current_song = Some(song);
                    self.sync_queue();
                    self.update_playing_song();
                    self.update_photo();
                    self.update_progress_title();
                    self.update_duration();
                    self.run_progress();
                }
            }
            Some(Status::Running | Status::Paused) => {}
            None => self.status = Some(Status::Stopped),
        };
    }

    pub fn run_tageditor(&mut self) {
        let mut tageditor: TagEditorActivity = TagEditorActivity::default();
        if let Some(Payload::One(Value::Str(node_id))) = self.view.get_state(COMPONENT_TREEVIEW) {
            let p: &Path = Path::new(node_id.as_str());
            if p.is_dir() {
                self.mount_error("directory doesn't have tag!");
                return;
            }

            let p = p.to_string_lossy();
            match Song::from_str(&p) {
                Ok(s) => {
                    // Get context
                    if let Some(ctx) = self.context.take() {
                        // Create activity
                        tageditor.on_create(ctx);
                        tageditor.init_by_song(&s);
                    } else {
                        error!("Failed to start TagEditorActivity: context is None");
                        return;
                    }
                }
                Err(e) => {
                    self.mount_error(format!("{}", e).as_ref());
                    return;
                }
            };
        }

        loop {
            // Draw activity
            tageditor.on_draw();
            tageditor.update_download_progress();
            tageditor.update_lyric_options();
            // Check if activity has terminated
            if let Some(ExitReason::Quit) = tageditor.will_umount() {
                // info!("SetupActivity terminated due to 'Quit'");
                break;
            }
            if let Some(ExitReason::NeedRefreshPlaylist(file)) = tageditor.will_umount() {
                // print!("{}", file);
                self.sync_playlist(Some(file));
                self.update_item_delete();
            }

            // Sleep for ticks
            sleep(Duration::from_millis(20));
        }
        // Destroy activity
        self.context = tageditor.on_destroy();

        self.update_photo();
        // drop(self.context.take());
    }
}

impl Activity for TermusicActivity {
    /// ### `on_create`
    ///
    /// `on_create` is the function which must be called to initialize the activity.
    /// `on_create` must initialize all the data structures used by the activity
    /// Context is taken from activity manager and will be released only when activity is destroyed
    fn on_create(&mut self, context: Context) {
        // Set context
        self.context = Some(context);
        // // Clear terminal
        if let Some(context) = self.context.as_mut() {
            context.clear_screen();
        }
        // // Put raw mode on enabled
        if let Err(err) = enable_raw_mode() {
            error!("Failed to enter raw mode: {}", err);
        }
        // // Init view
        self.init_setup();

        if let Err(err) = self.load_queue() {
            error!("Failed to save queue: {}", err);
        }
        self.status = Some(Status::Stopped);
    }

    /// ### `on_draw`
    ///
    /// `on_draw` is the function which draws the graphical interface.
    /// This function must be called at each tick to refresh the interface
    fn on_draw(&mut self) {
        // Context must be something
        if self.context.is_none() {
            return;
        }
        // Read one event
        // if let Some(context) = self.context.as_ref() {
        if let Ok(Some(event)) = crate::ui::inputhandler::InputHandler::read_event() {
            // Set redraw to true
            self.redraw = true;
            // Handle event
            let msg = self.view.on(event);
            self.update(msg);
        }
        // }
        // Redraw if necessary
        if self.redraw {
            // View
            self.view();
            // Redraw back to false
            self.redraw = false;
        }
    }

    /// ### `will_umount`
    ///
    /// `will_umount` is the method which must be able to report to the activity manager, whether
    /// the activity should be terminated or not.
    /// If not, the call will return `None`, otherwise return`Some(ExitReason)`
    fn will_umount(&self) -> Option<&ExitReason> {
        self.exit_reason.as_ref()
    }

    /// ### `on_destroy`
    ///
    /// `on_destroy` is the function which cleans up runtime variables and data before terminating the activity.
    /// This function must be called once before terminating the activity.
    /// This function finally releases the context
    fn on_destroy(&mut self) -> Option<Context> {
        if let Err(err) = self.save_queue() {
            error!("Failed to save queue: {}", err);
        }
        // Disable raw mode
        if let Err(err) = disable_raw_mode() {
            error!("Failed to disable raw mode: {}", err);
        }
        self.context.as_ref()?;
        // Clear terminal and return
        match self.context.take() {
            Some(mut ctx) => {
                ctx.clear_screen();
                Some(ctx)
            }
            None => None,
        }
    }
}
