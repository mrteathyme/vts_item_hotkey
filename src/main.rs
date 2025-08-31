use vtubestudio::data::{AvailableModelsRequest, CurrentModelRequest, HotkeyTriggerRequest, HotkeysInCurrentModelRequest, ItemListRequest};
use vtubestudio::{Client, ClientEvent, Error};

use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {


    let args: Vec<String> = env::args().collect();
    let model_name = args[1].clone();
    let item_name = args[2].clone();
    let hotkey_name = args[3].clone();

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

    let item_list_request = ItemListRequest {
        include_available_spots: false,
        include_item_instances_in_scene: true,
        include_available_item_files: false,
        only_items_with_file_name: Some(item_name),
        only_items_with_instance_id: None
    };
    
    let item_list = client.send(&item_list_request).await?;
    let instance = item_list.item_instances_in_scene[0].instance_id.clone();
    let model_list = AvailableModelsRequest {};
    let models = client.send(&model_list).await?;
    let mut model_id = String::new();
    for model in models.available_models {
        if model.model_name == model_name {
            model_id = model.model_id;
            break
        }
    };
    let hotkey_request = HotkeysInCurrentModelRequest {
        model_id: Some(model_id),
        live2d_item_file_name: Some(instance.clone())
    };

    let hotkeys = client.send(&hotkey_request).await?;
    let mut hotkey_id = String::new();
    for hotkey in hotkeys.available_hotkeys {
        if hotkey.name == hotkey_name {
            hotkey_id = hotkey.hotkey_id;
            break
        }
    } 
    let hotkey_request = HotkeyTriggerRequest {
        hotkey_id: hotkey_id,
        item_instance_id: Some(instance)
    };
    client.send(&hotkey_request).await?;

    Ok(())
}
