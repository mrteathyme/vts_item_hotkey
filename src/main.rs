#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::anyhow;
use egui::{ScrollArea, TextStyle};
use vtubestudio::data::{AvailableModelsRequest, CurrentModelRequest, HotkeyTriggerRequest, HotkeysInCurrentModelRequest, ItemListRequest, VtsFolderInfoRequest};
use vtubestudio::{Client, ClientEvent, Error};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

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
    let item_name = args[1].clone();
    let hotkey_name = args[2].clone();


    let stored_token = match std::fs::exists("token")? {
        true => {
            Some(std::fs::read_to_string("token")?)
        },
        false => None
    };

    let (mut client, mut events) = Client::builder()
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
    let mut items: BTreeMap<String, VtubeJson> = BTreeMap::new();
    let mut item_folder = items_folder.clone();
    let dir = std::fs::read_dir(items_folder)?;
    for entry in dir {
        let _entry = entry?;
        if *(_entry.file_name()) == *item_name {
            item_folder.push(_entry.file_name());
        }
        if _entry.file_type()?.is_dir() {
            let mut json_path = OsString::new();
            let dir = std::fs::read_dir(_entry.path())?;
            for entry in dir {
                let __entry = entry?;
                let filename = __entry.file_name().into_string().unwrap();
                let parts: Vec<&str> = filename.split(".").collect();
                if parts.len() <= 1 {
                    continue;
                }
                if parts[1] == "vtube" {
                    json_path = __entry.path().into_os_string();
                    items.insert(_entry.file_name().into_string().unwrap(), serde_json::from_str(&std::fs::read_to_string(json_path)?)?);
                }
            }
        }
    }

    let item_list_request = ItemListRequest {
        include_available_spots: false,
        include_item_instances_in_scene: true,
        include_available_item_files: false,
        only_items_with_file_name: None,
        only_items_with_instance_id: None
    };
    let item_list = client.send(&item_list_request).await?.item_instances_in_scene;
    let mut instances: HashMap<String, Vec<String>> = HashMap::new();
    for item in item_list {
        instances.entry(item.file_name).or_default().push(item.instance_id);
    }

    let _ = eframe::run_simple_native("test", options, move |ctx, frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            //let model_data = model_data.clone();
            let items = items.clone();
            let instance_map = instances.clone();
            ui.heading("Test app");
            let height = TextStyle::Body.resolve(ui.style()).size;
            ScrollArea::vertical().show_rows(ui,height,items.len(), |ui, row_range| {
                //ui.allocate_space([ui.available_width(),0.0].into());
                for i in row_range {
                    let (model_name, model_data) = items.iter().nth(i).unwrap();
                    if model_data.hotkeys.len() == 0 {continue;}
                    ui.collapsing(model_name, |ui| {
                    ScrollArea::vertical().id_salt(model_name).show_rows(ui,height,model_data.hotkeys.len(), |ui, row_range| {
                        //ui.allocate_space([ui.available_width(),0.0].into());
                        for i in row_range {
                            let Some(value) = model_data.hotkeys.get(i) else { continue; };
                            ui.label(format!("{:?}", value));
                            if let Some(instances) = instance_map.get(model_name) {
                                if ui.button("Play").clicked() {
                                    for instance in instances {
                                        let hotkey_request = HotkeyTriggerRequest {
                                            hotkey_id: value.clone().hotkey_id,
                                            item_instance_id: Some(instance.clone())
                                        };
                                        let client = client.clone();
                                        tokio::task::spawn(async move {
                                            let mut client = client.clone();
                                            let result = client.send(&hotkey_request).await.unwrap();
                                        });
                                    }
                                };
                            }
                        }
                    });});
                }
            });
        }); 
    });

    Ok(())
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
