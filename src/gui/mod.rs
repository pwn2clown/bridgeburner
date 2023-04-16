use std::{
    str::FromStr,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::{runtime::Builder, sync::mpsc};
use eframe::egui;
use egui_extras::{TableBuilder, Column};
use crate::proxy::{ProxyHandle, logs::ProxyLogs, certs::Identity};
//mod styles;
//use styles::*;


#[derive(Debug)]
struct ProxyMsg {
    pub handle: ProxyHandle,
    pub msg: ProxyCmd,
}

struct ProxyInfo {
    pub to_delete: bool,
    pub handle: ProxyHandle,
}

#[derive(Debug)]
enum ProxyCmd {
    Start,
}

#[derive(PartialEq)]
enum TabState {
    Empty,
    Proxy,
    Settings,
}

pub struct App {
    async_bridge_tx: mpsc::Sender<ProxyMsg>,
    proxy_table: Vec<ProxyInfo>,
    logs: Arc<Mutex<ProxyLogs>>,     //  TODO: use mutex type internally
    selected_tab: TabState,
    ca: Identity,
    //  TODO: Into widget state
    new_proxy_addr: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (bridge_send, mut bridge_recv) = mpsc::channel::<ProxyMsg>(1);
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let ca = Identity::certificate_authority()
            .expect("Failed to generate CA");

        std::thread::spawn(move || {
            rt.block_on(async move {
                while let Some(mut cmd) = bridge_recv.recv().await {
                    match cmd.msg {
                        ProxyCmd::Start => cmd.handle.serve().await,
                    }
                }
            });
        });

        Self {
            async_bridge_tx: bridge_send,
            proxy_table: Vec::new(),
            logs: Arc::new(Mutex::new(ProxyLogs::new())),
            selected_tab: TabState::Settings,
            ca: ca,
            new_proxy_addr: String::default(),
        }
    }
}

impl App {
    fn show_settings_menu(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_proxy_addr));

            if ui.button("+").clicked() {
                match SocketAddr::from_str(&self.new_proxy_addr) {
                    Ok(addr) => println!("{addr}"),
                    Err(err) => println!("bad addr"),
                }
            
                let addr = SocketAddr::from(([127, 0, 0, 1], 4444));
                let proxy_info = ProxyInfo {
                        to_delete: false,
                        handle: ProxyHandle::new(addr, self.ca.clone()),
                    };

                self.async_bridge_tx
                    .blocking_send(ProxyMsg {
                        handle: proxy_info.handle.clone(),
                        msg: ProxyCmd::Start,
                    })
                    .unwrap();

                self.proxy_table.push(proxy_info);
            }
        });

        for proxy_info in &mut self.proxy_table {
            ui.horizontal(|ui| {
                ui.label(format!("{}", proxy_info.handle.addr()));
                if ui.button("Delete").clicked() {
                    proxy_info.handle.stop();
                    proxy_info.to_delete = true;
                }
            });
        }

        //  TODO: Drop deleted proxies
        self.proxy_table.retain(|proxy_info| !proxy_info.to_delete);
    }

    fn show_proxy_logs(&mut self, ui: &mut egui::Ui) {
        TableBuilder::new(ui)
            //.columns(Column::auto().resizable(true), 4)
            .column(Column::exact(25.))
            .column(Column::exact(100.))
            .column(Column::exact(200.))
            .column(Column::exact(75.))
            .header(18., |mut header| {
                header.col(|ui| { ui.heading("Id"); });
                header.col(|ui| { ui.heading("Host"); });
                header.col(|ui| { ui.heading("Path"); });
                header.col(|ui| { ui.heading("Length"); });
            })
            .body(|mut body| { });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_panel").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.selected_tab, TabState::Settings, "Settings");
                    ui.selectable_value(&mut self.selected_tab, TabState::Proxy, "Proxy");
                    ui.selectable_value(&mut self.selected_tab, TabState::Empty, "Help");
                });
        });

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                match self.selected_tab {
                    TabState::Settings => self.show_settings_menu(ui),
                    TabState::Proxy => self.show_proxy_logs(ui),
                    _ => (),
                }
            });
    }
}
