#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::anyhow;
use bytes::Bytes;
use egui::{ScrollArea, TextStyle};
use vtubestudio::data::{AvailableModelsRequest, CurrentModelRequest, HotkeyTriggerRequest, HotkeysInCurrentModelRequest, ItemListRequest, VtsFolderInfoRequest};
use vtubestudio::{Client, ClientEvent, Error};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct LibraryFolders(HashMap<String, LibraryFolder>);

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct LibraryFolder {
    path: String,
    apps: HashMap<String, String>
}

#[derive(Deserialize, Debug, Clone)]
struct VtubeJson {
    #[serde(rename = "ModelID")]
    model_id: String,
    #[serde(rename = "Hotkeys")]
    hotkeys: Vec<Hotkey>
}

#[derive(Deserialize, Debug, Clone)]
struct Hotkey {
    #[serde(rename = "HotkeyID")]
    hotkey_id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "File")]
    file: String
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0,240.0]),
        ..Default::default()
    };

    let mut steam_data = SteamData::new(None,None);
    steam_data.update_directory()?;
    steam_data.update_libraries()?;
    let mut streaming_assets = steam_data.get_game_directory(1325860)?.unwrap();
    streaming_assets.push("VTube Studio_Data/StreamingAssets/");

    let args: Vec<String> = env::args().collect();
    let port = args[1].clone();

    let stored_token = match std::fs::exists("token")? {
        true => {
            Some(std::fs::read_to_string("token")?)
        },
        false => None
    };

    let (mut client, mut events) = Client::builder()
        .url(format!("ws://localhost:{port}"))
        .authentication("The plugin that fixes the bonk thing", "TeaThyme", None)
        .auth_token(stored_token)
        .build_tungstenite();

    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                ClientEvent::NewAuthToken(token) => {
                    println!("Got new token: {token}");
                    std::fs::write("token", token).unwrap();
                    
                },
                _ => println!("Received event: {:?}", event)
                } 
            }
        }
    );

    let folder_request = VtsFolderInfoRequest {};
    let folders = client.send(&folder_request).await?;
    let mut items_folder = streaming_assets.clone();
    items_folder.push(folders.items);
    let mut items: BTreeMap<String, ItemData> = BTreeMap::new();
    let dir = std::fs::read_dir(items_folder)?;
    for entry in dir {
        let _entry = entry?;
        if _entry.file_type()?.is_dir() {
            let dir = std::fs::read_dir(_entry.path())?;
            for entry in dir {
                let __entry = entry?;
                let filename = __entry.file_name().into_string().unwrap();
                let parts: Vec<&str> = filename.split(".").collect();
                if parts.len() <= 1 {
                    continue;
                }
                if parts[1] == "vtube" {
                    let json_path = __entry.path().into_os_string();
                    let icon_path = std::fs::read_dir(__entry.path().parent().unwrap())
                    .ok()
                    .and_then(|mut dir| dir.find_map(|e| {
                            let e = e.ok()?;
                            let path = e.path();
                            let ext = path.extension()?.to_str()?;
                            if ext == "png" || ext == "jpg" || ext == "jpeg" {
                                Some(path)
                            } else {
                                None
                            }
                        }
                    ));
                    let item_data = ItemData {
                        icon_data: icon_path.as_ref().and_then(|p| std::fs::read(p).ok()).map(bytes::Bytes::from),
                        icon_path: icon_path,
                        json:  serde_json::from_str(&std::fs::read_to_string(json_path)?)?
                    };
                    items.insert(_entry.file_name().into_string().unwrap(), item_data);
                }
            }
        }
    }



let ctx_shared: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
let ctx_bg = ctx_shared.clone();

let instances = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));
let instances_bg = instances.clone();
let mut client_bg = client.clone();

tokio::spawn(async move {
    loop {
        let item_list_request = ItemListRequest {
            include_available_spots: false,
            include_item_instances_in_scene: true,
            include_available_item_files: false,
            only_items_with_file_name: None,
            only_items_with_instance_id: None
        };
        if let Ok(item_list) = client_bg.send(&item_list_request).await {
            let mut new_instances: HashMap<String, Vec<String>> = HashMap::new();
            for item in item_list.item_instances_in_scene {
                new_instances.entry(item.file_name).or_default().push(item.instance_id);
            }
            *instances_bg.lock().unwrap() = new_instances;
            if let Some(ctx) = ctx_bg.lock().unwrap().as_ref() {
                ctx.request_repaint();
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
});

let _ = eframe::run_native(
    "test",
    options,
    Box::new(move |cc| {
        *ctx_shared.lock().unwrap() = Some(cc.egui_ctx.clone());
        Ok(Box::new(MyApp {
            items,
            instances,
            client,
            item_list: ItemList { view_state: ItemListView::Grid },
        }))
    })
);
    Ok(())
}

struct MyApp {
    items: BTreeMap<String, ItemData>,
    instances: Arc<Mutex<HashMap<String, Vec<String>>>>,
    client: Client,
    item_list: ItemList,
}

impl MyApp {
    fn context(&mut self) -> AppContext {
        AppContext {
            items: &self.items,
            instance_map: self.instances.lock().unwrap(),
            client: &mut self.client,
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui_extras::install_image_loaders(ctx);
        let mut item_list = std::mem::take(&mut self.item_list);
        let mut app_ctx = self.context();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Test app");
            item_list.show(ctx, ui, &mut app_ctx);
        });
        drop(app_ctx);
        self.item_list = item_list;
    }
}

struct AppContext<'a> {
    items: &'a BTreeMap<String, ItemData>,
    instance_map: std::sync::MutexGuard<'a, HashMap<String, Vec<String>>>,
    client: &'a mut Client,
}

#[derive(Debug, Clone)]
struct ItemData {
    icon_path: Option<PathBuf>,
    icon_data: Option<Bytes>,
    json: VtubeJson
}

#[derive(Default, Clone, Debug)]
enum ItemListView {
    #[default]
    List,
    Grid,
    Detail { selected: String, previous: Box<ItemListView> },
}
#[derive(Default)]
struct ItemList {
    view_state: ItemListView
}

impl ItemList {
    fn show(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, app_ctx: &mut AppContext) -> egui::Response {
        let view_state = self.view_state.clone();
        ui.scope(|ui| {
            match &view_state {
                ItemListView::List => self.render_list(ui, app_ctx),
                ItemListView::Grid => self.render_grid(ui, app_ctx),
                ItemListView::Detail { selected, previous } => {
                    match previous.as_ref() {
                        ItemListView::List => self.render_list(ui, app_ctx),
                        ItemListView::Grid => self.render_grid(ui, app_ctx),
                        _ => {}
                    }
                    let screen = ctx.screen_rect();
                    let modal = egui::Modal::new("item_details".into())
                        .area(egui::Area::new("item_details".into()).fixed_pos(egui::pos2(screen.width() * 0.1, screen.height() * 0.1)))
                        .show(ctx, |ui| {
                            ui.set_min_size(egui::vec2(screen.width() * 0.8, screen.height() * 0.8));
                            ui.set_max_width(screen.width() * 0.8);
                            self.render_modal(selected.clone(), ui, app_ctx);
                        });
                    if modal.should_close() {
                        self.view_state = *previous.clone();
                    }
                }
            }
        }).response
    }

    fn render_modal(&mut self, model_name: String, ui: &mut egui::Ui, ctx: &mut AppContext) {
        let Some(model_data) = ctx.items.get(&model_name) else { return; };

        let available = ui.available_size();
        let icon_size = (available.x * available.y).sqrt() * 0.35;
        let padding = icon_size * 0.1;
        let title_font_size = icon_size * 0.15;

        ui.horizontal(|ui| {
            egui::Frame::NONE
                .inner_margin(padding)
                .show(ui, |ui| {
                    if let Some(bytes) = &model_data.icon_data {
                        let uri = format!("bytes://{}", model_data.icon_path.as_ref().map(|p| p.to_string_lossy()).unwrap_or_default());
                        let image = egui::ImageSource::Bytes {
                            uri: uri.into(),
                            bytes: egui::load::Bytes::from(bytes.to_vec())
                        };
                        ui.add(egui::Image::new(image).fit_to_exact_size(egui::vec2(icon_size, icon_size)));
                    }
                });
            ui.vertical(|ui| {
                ui.add_space(icon_size / 2.0 - icon_size * 0.1);
                ui.heading(egui::RichText::new(&model_name).size(title_font_size));
            });
        });

        ui.add_space(padding);
        ui.separator();
        ui.add_space(padding);

        egui::Frame::NONE
            .inner_margin(padding)
            .show(ui, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(padding, padding);
                        for hotkey in &model_data.json.hotkeys {
                            if let Some(instances) = ctx.instance_map.get(&model_name) {
                                if ui.add_sized(
                                    egui::vec2(icon_size * 0.6, icon_size * 0.3),
                                    egui::Button::new(egui::RichText::new(&hotkey.name).size(icon_size * 0.08))
                                ).clicked() {
                                    for instance in instances {
                                        let hotkey_request = HotkeyTriggerRequest {
                                            hotkey_id: hotkey.hotkey_id.clone(),
                                            item_instance_id: Some(instance.clone())
                                        };
                                        let client = ctx.client.clone();
                                        tokio::task::spawn(async move {
                                            let mut client = client.clone();
                                            client.send(&hotkey_request).await.unwrap();
                                        });
                                    }
                                }
                            }
                        }
                    });
                });
            });
    }

    fn render_grid(&mut self, ui: &mut egui::Ui, ctx: &mut AppContext) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (model_name, model_data) in ctx.items {
                    if model_data.json.hotkeys.len() == 0 { continue; }
                    if let Some(bytes) = &model_data.icon_data {
                        let uri = format!("bytes://{}", model_data.icon_path.as_ref().map(|p| p.to_string_lossy()).unwrap_or_default());
                        let image = egui::ImageSource::Bytes {
                            uri: uri.into(),
                            bytes: egui::load::Bytes::from(bytes.to_vec())
                        };
                    if ui.add(egui::ImageButton::new(egui::Image::new(image).fit_to_exact_size(egui::vec2(128.0, 128.0)))).clicked() {
                        println!("clicked: {}", model_name);
                        self.view_state = ItemListView::Detail { selected: model_name.clone(), previous: Box::new(self.view_state.clone()) };
                    }
                    }
                }
            });
        });
    }

    fn render_list(&mut self, ui: &mut egui::Ui, ctx: &mut AppContext) {
        ScrollArea::vertical().show(ui, |ui| {
                //ui.allocate_space([ui.available_width(),0.0].into());
                //for i in row_range {
                for (model_name, model_data) in ctx.items {
                    //let (model_name, model_data) = items.iter().nth(i).unwrap();
                    if model_data.json.hotkeys.len() == 0 {continue;}
                    ui.horizontal(|ui| {
                    if let Some(bytes) = &model_data.icon_data {
                        let uri = format!("bytes://{}", model_data.icon_path.as_ref().map(|p| p.to_string_lossy()).unwrap_or_default());
                        let image = egui::ImageSource::Bytes { 
                            uri: uri.into(), 
                            bytes: egui::load::Bytes::from(bytes.to_vec())
                        };
                        ui.add(egui::Image::new(image).fit_to_exact_size(egui::vec2(128.0, 128.0)).bg_fill(egui::Color32::WHITE));
                    }
                    ui.vertical_centered(|ui| {
                        ui.add_space(128.0/2.0);
                    ui.collapsing(model_name, |ui| {
                        ScrollArea::vertical().id_salt(&model_name).show_rows(ui,128.0,model_data.json.hotkeys.len(), |ui, row_range| {
                            //ui.allocate_space([ui.available_width(),0.0].into());
                            for i in row_range {
                                let Some(value) = model_data.json.hotkeys.get(i) else { continue; };
                                ui.label(format!("{:?}", value));
                                if let Some(instances) = ctx.instance_map.get(model_name) {
                                    if ui.button("Play").clicked() {
                                        for instance in instances {
                                            let hotkey_request = HotkeyTriggerRequest {
                                                hotkey_id: value.clone().hotkey_id,
                                                item_instance_id: Some(instance.clone())
                                            };
                                            let client = ctx.client.clone();
                                            tokio::task::spawn(async move {
                                                let mut client = client.clone();
                                                let result = client.send(&hotkey_request).await.unwrap();
                                            });
                                        }
                                    };
                                }
                            }
                        });
                    });});});
                }
            });
    }

    fn show_item(&mut self, ui: &mut egui::Ui, model_name: &str, model_data: &ItemData) -> egui::Response {
        ui.horizontal(|ui| {
            // your rendering code
        }).response
    }
}





#[derive(Debug)]
struct SteamData {
    directory: Option<PathBuf>,
    libraries: Option<Vec<SteamLibrary>>
}

#[derive(Clone, Debug)]
struct SteamLibrary {
    directory: PathBuf,
    games: HashMap<u32, String>
}

impl SteamData {
    fn new(directory: Option<PathBuf>, libraries: Option<Vec<SteamLibrary>>) -> SteamData {
        Self {
            directory,
            libraries
        }    
    }
    fn find_directory() -> anyhow::Result<PathBuf> {
        #[cfg(target_os = "linux")] {
            let home_dir = env::home_dir().unwrap();
            let mut local = home_dir.clone();
            let mut steam = home_dir.clone();
            let mut flatpak = home_dir.clone();
            drop(home_dir);
            local.push(".local/share/Steam");
            steam.push(".steam/steam");
            flatpak.push(".var/app/com.valvesoftware.Steam/data/Steam");
            if std::fs::exists(&steam)? {
                return Ok(steam);
            } else if std::fs::exists(&local)? {
                return Ok(local);
            } else if std::fs::exists(&flatpak)? {
                return Ok(flatpak);
            } else {
                use anyhow::anyhow;

                return Err(anyhow!("No steam install found"));
            }
        }

        #[cfg(target_os = "windows")] {
            use anyhow::anyhow;
            let program_files_x86 = env::var("PROGRAMFILES(X86)")
                .unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());
            let program_files = env::var("PROGRAMFILES")
                .unwrap_or_else(|_| "C:\\Program Files".to_string());

            let mut steam_x86 = PathBuf::from(program_files_x86);
            steam_x86.push("Steam");
            let mut steam = PathBuf::from(program_files);
            steam.push("Steam");

            if std::fs::exists(&steam_x86)? {
                return Ok(steam_x86);
            } else if std::fs::exists(&steam)? {
                return Ok(steam);
            } else {
                return Err(anyhow!("No steam install found"));
            }
        }
    }
    fn update_directory(&mut self) -> anyhow::Result<()> {
        self.directory = Some(Self::find_directory()?);
        Ok(())
    }
    fn find_libraries(steam_dir: PathBuf) -> anyhow::Result<Vec<SteamLibrary>> {
        let mut library_dir = steam_dir;
        library_dir.push("steamapps/libraryfolders.vdf");
        let libraryvdf = std::fs::read_to_string(library_dir)?;
        let steam_libraries: LibraryFolders = keyvalues_serde::from_str(&libraryvdf)?;
        let mut libraries = vec![];
        for (_, library) in steam_libraries.0 {
            let mut steam_library = SteamLibrary {
                directory: library.path.into(),
                games: HashMap::new()
            };
            for (id, _) in library.apps {
                steam_library.games.insert(id.parse()?, "".to_string());
            }
            libraries.push(steam_library);
        }
        Ok(libraries)
    }
    fn update_libraries(&mut self) -> anyhow::Result<()> {
        match self.directory.clone() {
            Some(dir) => {self.libraries = Some(Self::find_libraries(dir)?); 
                Ok(())
            },
            None => Err(anyhow!("SteamDirectory not set before discovering libraries"))
        }
    }

    fn get_game_directory(&mut self, game_id: u32) -> anyhow::Result<Option<PathBuf>> {
        if let Some(libraries) = self.libraries.clone() {
            for library in libraries {
                for (game, name) in library.games {
                    if game == game_id {
                        let mut directory = library.directory;
                        directory.push("steamapps");
                        let mut manifest_path = directory.clone();
                        manifest_path.push(format!("appmanifest_{}.acf", game_id));
                        let manifest: SteamAppManifest = keyvalues_serde::from_str(&std::fs::read_to_string(manifest_path)?)?;
                        directory.push(format!("common/{}",manifest.installdir));
                        return Ok(Some(directory));
                    }
                }
            } 
        };
        Ok(None)
    }
}

#[derive(Deserialize, Debug, Clone)]
struct SteamAppManifest {
    name: String,
    installdir: String
}
