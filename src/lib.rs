use {
    std::{
        sync::Mutex,
        cell::RefCell,
    },
    rengo_common::networking::*,
    rengo_common::logic::{
        Position,
        Move,
        InvalidMove,
        Stone,
    },
    wasm_bindgen::{
        prelude::*,
        JsCast,
    },
    web_sys::{
        Blob,
        FileReader,
        HtmlElement,
        HtmlCanvasElement,
        HtmlImageElement,
        KeyboardEvent,
        CanvasRenderingContext2d,
        HtmlInputElement,
        WebSocket,
        MouseEvent,
        MessageEvent,
        ProgressEvent,
    },
};

macro_rules! console_log {
    ($($t:tt)*) => (web_sys::console::log_1(&format!($($t)*).into()))
}

type JsResult<T> = Result<T, JsValue>;
type JsError = JsResult<()>;
type JsClosureNone = Closure<dyn FnMut() -> JsError>;
type JsClosure<T> = Closure<dyn FnMut(T) -> JsError>;

unsafe impl Send for Game {}

struct Game {
    ws: Option<WebSocket>,
    room: Option<Room>,
    inner_begin: Option<f64>,
    inner_size: Option<f64>,
    line_space: Option<f64>,
}

impl Game {
    fn login() -> JsError {
        Game::on_button_login_submit()?;
        Game::on_window_resize()?;
        Game::set_mouse_move()?;
        Game::set_mouse_click()?;
        Game::set_mouse_out()?;
        Game::set_pass_button()?;
        Game::set_quit_button()?;
        Game::set_message_button()?;
        Game::set_enter_key()?;
        Ok(())
    }

    fn on_ws_open() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_server = document.get_element_by_id("loginServer")
            .unwrap();

        login_server.set_class_name("form-control");

        let login_server_error_res = document
            .get_element_by_id("loginServerError");

        if let Some(login_server_error) = login_server_error_res {
            login_server_error.remove();
        }

        let login_username = document.get_element_by_id("loginUsername")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        let login_room = document.get_element_by_id("loginRoom")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;

        let login_message = ClientMessage::Login(login_username.value(), login_room.value());
        Game::ws_send_message(&login_message)?;

        Ok(())
    }

    fn on_ws_error() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_server = document.get_element_by_id("loginServer")
            .unwrap();

        login_server.set_class_name("form-control is-invalid");

        let login_server_error = document
            .get_element_by_id("loginServerError");

        if let None = login_server_error {
            let login_server_error = document.create_element("div")?
                .dyn_into::<HtmlElement>()?;
            login_server_error.set_id("loginServerError");
            login_server_error.set_class_name("invalid-feedback");
            login_server_error.set_inner_text("Could not join server: Unable to connect to server");

            let login_server_form = document.get_element_by_id("loginServerForm")
                .unwrap();

            login_server_form.append_child(&login_server_error)?;
        } else {
            login_server_error
                .unwrap()
                .dyn_into::<HtmlElement>()?
                .set_inner_text("Could not join server: Unable to connect to server");
        }

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .ws = None;

        Ok(())
    }

    fn on_ws_close() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let game = document.get_element_by_id("game")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        game.set_hidden(true);

        let login = document.get_element_by_id("login")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        login.set_hidden(false);

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .ws = None;

        Game::reset_game()?;

        Ok(())
    }

    fn reset_game() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let game_status_room_players = document.get_element_by_id("gameStatusRoomPlayers")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let game_status_chat_messages = document.get_element_by_id("gameStatusChatMessages")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        game_status_room_players.set_inner_html("");
        game_status_chat_messages.set_inner_html("");

        Ok(())
    }

    fn on_button_login_submit() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let server_input_box = document.get_element_by_id("loginServer")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        
        server_input_box.set_value("wss://server.nicbk.com/rengo");

        let button_submit = document.get_element_by_id("loginSubmit")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let on_button_submit = Closure::wrap(Box::new(move || {
            let mut ws_exists = false;

            if let Some(_) = STATE.lock()
                .unwrap()
                .borrow()
                .ws
            {
                ws_exists = true;
            }

            if ! ws_exists {
                let login_server = document.get_element_by_id("loginServer")
                    .unwrap()
                    .dyn_into::<HtmlInputElement>()?;

                let ws_res = WebSocket::new(&login_server.value());

                if let Ok(ws) = ws_res {
                    STATE.lock()
                        .unwrap()
                        .borrow_mut()
                        .ws = Some(ws);

                    let ws_onerror = Closure::wrap(Box::new(|| {
                        Game::on_ws_error()?;
                        Ok::<(), JsValue>(())
                    }) as Box<dyn FnMut() -> JsError>);

                    let ws_onopen = Closure::wrap(Box::new(|| {
                        Game::on_ws_open()?;
                        Ok::<(), JsValue>(())
                    }) as Box<dyn FnMut() -> JsError>);

                    let ws_ondecode = Closure::wrap(Box::new(|e: ProgressEvent| {
                        let reader: FileReader = e.target()
                            .unwrap()
                            .dyn_into()?;
                        let result = reader.result()?;
                        let buf = js_sys::Uint8Array::new(&result);
                        let mut data = vec![0; buf.length() as usize];
                        buf.copy_to(&mut data);
                        Game::on_ws_message(&data)?;
                        Ok::<(), JsValue>(())
                    }) as Box<dyn FnMut(ProgressEvent) -> JsError>);
                    
                    let ws_onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
                        let blob = e.data().dyn_into::<Blob>()?;
                        let fr = FileReader::new()?;
                        fr.add_event_listener_with_callback("load", &ws_ondecode.as_ref().unchecked_ref())?;
                        fr.read_as_array_buffer(&blob)?;
                        Ok::<(), JsValue>(())
                    }) as Box<dyn FnMut(MessageEvent) -> JsError>);

                    let ws_onclose = Closure::wrap(Box::new(|| {
                        Game::on_ws_close()?;
                        Ok::<(), JsValue>(())
                    }) as Box<dyn FnMut() -> JsError>);

                    Game::ws_add_event_listener_none("error", &ws_onerror)?;
                    Game::ws_add_event_listener_none("open", &ws_onopen)?;
                    Game::ws_add_event_listener("message", &ws_onmessage)?;
                    Game::ws_add_event_listener_none("close", &ws_onclose)?;

                    ws_onerror.forget();
                    ws_onopen.forget();
                    ws_onmessage.forget();
                    ws_onclose.forget();
                } else {
                    Game::on_ws_error()?;
                }
            } else {
                Game::on_ws_open()?;
            }

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        button_submit.set_onclick(Some(on_button_submit.as_ref().unchecked_ref()));

        on_button_submit.forget();

        Ok(())
    }

    fn set_mouse_move() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let canvas = document.get_element_by_id("gameBoard")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let mouse_move_handler = Closure::wrap(Box::new(|e: MouseEvent| {
            Game::on_mouse_move(e.offset_x(), e.offset_y())?;
            Game::render()?;
            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut(MouseEvent) -> JsError>);

        canvas.set_onmousemove(Some(mouse_move_handler.as_ref().unchecked_ref()));

        mouse_move_handler.forget();

        Ok(())
    }

    fn set_mouse_click() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let canvas = document.get_element_by_id("gameBoard")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let mouse_click_handler = Closure::wrap(Box::new(|e: MouseEvent| {
            Game::on_mouse_click(e.offset_x(), e.offset_y())?;
            Game::render()?;
            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut(MouseEvent) -> JsError>);

        canvas.set_onmouseup(Some(mouse_click_handler.as_ref().unchecked_ref()));

        mouse_click_handler.forget();

        Ok(())
    }

    fn set_mouse_out() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let canvas = document.get_element_by_id("gameBoard")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let mouse_out_handler = Closure::wrap(Box::new(|| {
            Game::on_mouse_out()?;
            Game::render()?;
            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        canvas.set_onmouseout(Some(mouse_out_handler.as_ref().unchecked_ref()));

        mouse_out_handler.forget();

        Ok(())
    }

    fn get_window_size() -> u32 {
        let window = web_sys::window()
            .unwrap();

        let mut inner_width = window.inner_width()
            .unwrap()
            .as_f64()
            .unwrap() * (2_f64 / 3_f64);

        let inner_height = window.inner_height()
            .unwrap()
            .as_f64()
            .unwrap() * (9_f64 / 10_f64);

        if inner_width > inner_height {
            inner_width = inner_height;
        }

        inner_width as u32
    }

    fn on_window_resize() -> JsError {
        let window = web_sys::window()
            .unwrap();

        let on_resize = Closure::wrap(Box::new(|| {
            BOARD_SIZE.lock()
                .unwrap()
                .replace(Game::get_window_size());

            Game::status_bar_size()?;
            Game::render()?;

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        window.set_onresize(Some(on_resize.as_ref().unchecked_ref()));

        on_resize.forget();

        Ok(())
    }

    fn ws_add_event_listener<T>(event: &str, handler: &JsClosure<T>) -> JsError {
        STATE.lock()
            .unwrap()
            .borrow()
            .ws
            .as_deref()
            .unwrap()
            .add_event_listener_with_callback(event, handler.as_ref().unchecked_ref())?;

        Ok(())
    }

    fn ws_add_event_listener_none(event: &str, handler: &JsClosureNone) -> JsError {
        STATE.lock()
            .unwrap()
            .borrow()
            .ws
            .as_deref()
            .unwrap()
            .add_event_listener_with_callback(event, handler.as_ref().unchecked_ref())?;

        Ok(())
    }

    fn ws_send_message(message: &ClientMessage) -> JsError {
        let message_encoded = bincode::serialize(message)
            .map_err(|e| JsValue::from_str(
                    &format!("Could not serialize ClientMessage: {}", e)))?;
        STATE.lock()
            .unwrap()
            .borrow()
            .ws
            .as_ref()
            .unwrap()
            .send_with_u8_array(&message_encoded)?;

        Ok(())
    }

    fn on_ws_message(message: &[u8]) -> JsError {
        let server_message = bincode::deserialize::<ServerMessage>(message)
            .map_err(|e| JsValue::from_str(
                    &format!("Could not deserialize ServerMessage: {}", e)))?;

        match server_message {
            ServerMessage::LoginResponse(result) =>
                Game::on_login_response(result)?,
            ServerMessage::RoomCreateResponse(result) =>
                Game::on_room_create_response(result)?,
            ServerMessage::PlaceResponse(result) =>
                Game::on_place_response(result)?,
            ServerMessage::PlayerAdd(player) =>
                Game::on_player_add(player)?,
            ServerMessage::PlayerRemove(username) =>
                Game::on_player_remove(username)?,
            ServerMessage::NextTurn(username) =>
                Game::on_next_turn(username)?,
            ServerMessage::Chat(message) =>
                Game::on_chat_message_received(message)?,
            ServerMessage::AlreadyLoggedIn =>
                Game::on_player_already_logged_in()?
        }

        Ok(())
    }

    fn on_login_response(result: Result<Room, LoginError>) -> JsError {
        Game::login_form_reset()?;

        match result {
            Ok(room) => Game::on_login_response_success(room)?,

            Err(login_error) => match login_error {
                LoginError::RoomFull =>
                    Game::on_login_response_room_full()?,
                LoginError::UsernameTaken =>
                    Game::on_login_response_username_taken()?,
                LoginError::RoomNameTooLong =>
                    Game::on_login_response_room_name_too_long()?,
                LoginError::UsernameTooLong =>
                    Game::on_login_response_username_too_long()?,
                LoginError::RoomDoesNotExist(room_name) =>
                    Game::on_login_response_room_does_not_exist(room_name)?,
            }
        }

        Ok(())
    }

    fn on_login_response_success(room: Room) -> JsError {
        let current_player = room.current_player.clone();

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .room = Some(room);

        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_form = document.get_element_by_id("login")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        login_form.set_hidden(true);

        let room_el = document.get_element_by_id("loginRoom")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;


        Game::status_bar_size()?;
        Game::status_bar_header(room_el.value())?;

        let current_player_el = document.get_element_by_id(&format!("player-{}", current_player))
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        current_player_el.style()
            .set_property("background-color", "grey")?;
        
        Game::render()?;

        Ok(())
    }

    fn on_login_response_room_full() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_room = document.get_element_by_id("loginRoom")
            .unwrap();

        login_room.set_class_name("form-control is-invalid");

        let login_room_error = document
            .get_element_by_id("loginRoomError");

        if let None = login_room_error {
            let login_room_error = document.create_element("div")?
                .dyn_into::<HtmlElement>()?;
            login_room_error.set_id("loginRoomError");
            login_room_error.set_class_name("invalid-feedback");
            login_room_error.set_inner_text("Could not join room: Room is full");

            let login_room_form = document.get_element_by_id("loginRoomForm")
                .unwrap();

            login_room_form.append_child(&login_room_error)?;
        } else {
            login_room_error
                .unwrap()
                .dyn_into::<HtmlElement>()?
                .set_inner_text("Could not join room: Room is full");
        }

        Ok(())
    }

    fn on_login_response_username_taken() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_username = document.get_element_by_id("loginUsername")
            .unwrap();

        login_username.set_class_name("form-control is-invalid");

        let login_username_error = document
            .get_element_by_id("loginUsernameError");

        if let None = login_username_error {
            let login_username_error = document.create_element("div")?
                .dyn_into::<HtmlElement>()?;
            login_username_error.set_id("loginUsernameError");
            login_username_error.set_class_name("invalid-feedback");
            login_username_error.set_inner_text("Invalid username: Username is taken");

            let login_username_form = document.get_element_by_id("loginUsernameForm")
                .unwrap();

            login_username_form.append_child(&login_username_error)?;
        } else {
            login_username_error
                .unwrap()
                .dyn_into::<HtmlElement>()?
                .set_inner_text("Invalid username: Username is taken");
        }

        Ok(())
    }

    fn on_login_response_room_name_too_long() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_room = document.get_element_by_id("loginRoom")
            .unwrap();

        login_room.set_class_name("form-control is-invalid");

        let login_room_error = document
            .get_element_by_id("loginRoomError");

        if let None = login_room_error {
            let login_room_error = document.create_element("div")?
                .dyn_into::<HtmlElement>()?;
            login_room_error.set_id("loginRoomError");
            login_room_error.set_class_name("invalid-feedback");
            login_room_error.set_inner_text("Room name too long: Max room name is 8 characters");

            let login_room_form = document.get_element_by_id("loginRoomForm")
                .unwrap();

            login_room_form.append_child(&login_room_error)?;
        } else {
            login_room_error
                .unwrap()
                .dyn_into::<HtmlElement>()?
                .set_inner_text("Room name too long: Max room name is 8 characters");
        }

        Ok(())
    }

    fn on_login_response_username_too_long() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_username = document.get_element_by_id("loginUsername")
            .unwrap();

        login_username.set_class_name("form-control is-invalid");

        let login_username_error = document
            .get_element_by_id("loginUsernameError");

        if let None = login_username_error {
            let login_username_error = document.create_element("div")?
                .dyn_into::<HtmlElement>()?;
            login_username_error.set_id("loginUsernameError");
            login_username_error.set_class_name("invalid-feedback");
            login_username_error.set_inner_text("Username too long: Max length is 16 characters");

            let login_username_form = document.get_element_by_id("loginUsernameForm")
                .unwrap();

            login_username_form.append_child(&login_username_error)?;
        } else {
            login_username_error
                .unwrap()
                .dyn_into::<HtmlElement>()?
                .set_inner_text("Username too long: Max length is 16 characters");
        }
        Ok(())
    }

    fn on_login_response_room_does_not_exist(room_name: String) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let header_message = document.create_element("h1")?
            .dyn_into::<HtmlElement>()?;
        header_message.set_inner_text("Create The Room");

        let status_header = document.get_element_by_id("statusHeader")
            .unwrap();
        status_header.append_child(&header_message)?;

        let status_paragraph = document.create_element("p")?
            .dyn_into::<HtmlElement>()?;
        status_paragraph.set_inner_text(&format!(r#"The room "{}" does not exist yet. Please create it below."#, room_name));

        let status_form = document.create_element("form")?;
        status_form.set_inner_html(r#"
            <br>
            <div class="form-group">
                <div class="form-row">
                    <div class="col-4">
                        <label>Board Length</label>
                    </div>
                    <div class="col-8">
                        <input type="text" class="form-control" id="boardLength" placeholder="Board Length">
                    </div>
                </div>
                <div class="invalid-feedback" id="invalidDimensions"></div>
            </div>
            <br>
            <div class="form-group">
                <div class="form-row">
                    <div class="col-4">
                        <label for="roomCapacity">Room Capacity</label>
                    </div>
                    <div class="col-8">
                        <input type="text" class="form-control" id="roomCapacity" placeholder="Room Capacity">
                    </div>
                </div>
                <div class="invalid-feedback" id="invalidCapacity"></div>
            </div>"#);

        let status_body = document.get_element_by_id("statusBody")
            .unwrap();
        status_body.append_child(&status_paragraph)?;
        status_body.append_child(&status_form)?;


        let status_footer = document.get_element_by_id("statusFooter")
            .unwrap();

        let quit_button = document.create_element("button")?
            .dyn_into::<HtmlElement>()?;
        quit_button.set_class_name("btn btn-secondary");
        quit_button.set_attribute("type", "button")?;
        quit_button.set_attribute("data-dismiss", "modal")?;
        quit_button.set_inner_text("Quit");

        let create_button = document.create_element("button")?
            .dyn_into::<HtmlElement>()?;
        create_button.set_class_name("btn btn-primary");
        create_button.set_attribute("type", "button")?;
        create_button.set_inner_text("Create");

        let create_button_handle = Closure::wrap(Box::new(move || {
            Game::on_room_create_button_submit(room_name.as_str())?;
            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        let quit_button_handle = Closure::wrap(Box::new(move || {
            Game::status_modal_reset()?;

            STATE.lock()
                .unwrap()
                .borrow()
                .ws
                .as_ref()
                .unwrap()
                .close()?;

            STATE.lock()
                .unwrap()
                .borrow_mut()
                .ws = None;

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        create_button.set_onclick(Some(create_button_handle.as_ref().unchecked_ref()));
        quit_button.set_onclick(Some(quit_button_handle.as_ref().unchecked_ref()));

        create_button_handle.forget();
        quit_button_handle.forget();

        status_footer.append_child(&quit_button)?;
        status_footer.append_child(&create_button)?;

        let status = document.get_element_by_id("status")
            .unwrap();

        status.set_attribute("data-backdrop", "static")?;
        status.set_attribute("data-keyboard", "false")?;

        let show_modal = js_sys::Function::new_with_args("name", "$(name).modal('show')");
        show_modal.call1(&JsValue::null(), &JsValue::from_str("#status"))?;

        Game::login_form_reset()?;

        Ok(())
    }

    fn on_room_create_button_submit(room_name: &str) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let capacity_err = document.get_element_by_id("invalidCapacity")
            .unwrap()
            .dyn_into::<HtmlElement>()?;
        capacity_err.set_inner_text("");

        let dimensions_err = document.get_element_by_id("invalidDimensions")
            .unwrap()
            .dyn_into::<HtmlElement>()?;
        dimensions_err.set_inner_text("");

        let mut error = false;

        let capacity = document.get_element_by_id("roomCapacity")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;

        capacity.set_class_name("form-control");

        let capacity = capacity.value() 
            .parse::<u8>()
            .map_err(|e| JsValue::from_str(&format!("Unable to parse roomCapacity: {}", e)));

        if let Err(_) = capacity {
            let capacity_el = document.get_element_by_id("roomCapacity")
                .unwrap()
                .dyn_into::<HtmlElement>()?;
            capacity_el.set_class_name("form-control is-invalid");

            let capacity_err = document.get_element_by_id("invalidCapacity")
                .unwrap()
                .dyn_into::<HtmlElement>()?;
            capacity_err.set_inner_text(r#"Invalid Capacity. Enter a positive integer such as "4"."#);
            error = true;
        }

        let board_size = document.get_element_by_id("boardLength")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        board_size.set_class_name("form-control");

        let board_size = board_size.value()
            .parse::<u8>()
            .map_err(|e| JsValue::from_str(&format!("Unable to parse boardLength: {}", e)));

        if let Err(_) = board_size {
            let dimensions_el = document.get_element_by_id("boardLength")
                .unwrap()
                .dyn_into::<HtmlElement>()?;
            dimensions_el.set_class_name("form-control is-invalid");

            let dimensions_err = document.get_element_by_id("invalidDimensions")
                .unwrap()
                .dyn_into::<HtmlElement>()?;
            dimensions_err.set_inner_text(r#"Invalid Dimensions. Enter a positive integer such as "9"."#);
            error = true;
        }

        if error {
            return Err(JsValue::from_str(&"Invalid input"));
        }

        let create_room = ClientMessage::RoomCreate(String::from(room_name), capacity?, board_size.clone()?, board_size?);

        Game::ws_send_message(&create_room)?;
        Ok(())
    }

    fn status_modal_reset() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let status_modal = document.get_element_by_id("status")
            .unwrap();

        status_modal.remove();

        let status_modal = document.create_element("div")?;

        status_modal.set_inner_html(r#"
            <div class="modal fade" id="status" tabindex="-1" role="dialog" aria-labelledby="statusLabel" aria-hidden="true">
                <div class="modal-dialog">
                    <div class="modal-content">
                        <div class="modal-header" id="statusHeader">
                        </div>
                        <div class="modal-body" id="statusBody">
                        </div>
                        <div class="modal-footer" id="statusFooter">
                        </div>
                    </div>
                </div>
            </div>"#);

        document.body()
            .unwrap()
            .append_child(&status_modal)?;

        Ok(())
    }

    fn on_room_create_response(result: Result<Option<Room>, RoomCreateError>) -> JsError {
        match result {
            Ok(room_result) => match room_result {
                Some(_) => (),
                None =>
                    Game::on_room_create_response_success()?,
            }

            Err(room_create_error) => match room_create_error {
                RoomCreateError::RoomNameTooLong =>
                    Game::on_room_create_response_room_name_too_long()?,
                RoomCreateError::RoomNameTaken =>
                    Game::on_room_create_response_room_name_taken()?,
            }
        }

        Ok(())
    }

    fn on_room_create_response_success() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let show_modal = js_sys::Function::new_with_args("name", "$(name).modal('hide')");
        show_modal.call1(&JsValue::null(), &JsValue::from_str("#status"))?;

        Game::status_modal_reset()?;

        let username = document.get_element_by_id("loginUsername")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        let username = username.value();
        
        let room = document.get_element_by_id("loginRoom")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        let room = room.value();

        let login_message = ClientMessage::Login(username, room);

        Game::ws_send_message(&login_message)?;

        Ok(())
    }

    fn on_room_create_response_room_name_too_long() -> JsError {
        // UNHANDLED
        Ok(())
    }

    fn on_room_create_response_room_name_taken() -> JsError {
        // UNHANDLED
        Ok(())
    }

    fn on_place_response(result: Result<Move<u8>, InvalidMove>) -> JsError {
        match result {
            Err(_invalid_move) => {
                // UNHANDLED
            }

            Ok(action) => Game::on_place_response_success(action)?
        }
        Ok(())
    }

    fn on_place_response_success(action: Move<u8>) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        match action.0 {
            Some(stone_move) => {
                Game::move_stone(stone_move.0, stone_move.1)?;

                let player_table = document.get_element_by_id("gameStatusRoomPlayers")
                    .unwrap()
                    .dyn_into::<HtmlElement>()?;

                let players = player_table.children();

                for player_i in 0..players.length() {
                    let player = players.item(player_i)
                        .unwrap()
                        .dyn_into::<HtmlElement>()?;

                    player.style()
                        .set_property("background-color", "#ffffff")?;
                }
            }

            None => {
                let username = action.1.unwrap();

                let username_el = document.get_element_by_id(&format!("player-{}", username))
                    .unwrap()
                    .dyn_into::<HtmlElement>()?;

                username_el.style()
                    .set_property("background-color", "red")?;
            }
        }
        Ok(())
    }

    fn move_stone(position: Position<u8>, stone: Option<Stone>) -> JsError {
        STATE.lock()
            .unwrap()
            .borrow_mut()
            .room
            .as_mut()
            .unwrap()
            .board
            .stones[position.y() as usize][position.x() as usize] = stone;

        Game::render()?;

        Ok(())
    }

    fn on_player_add(player: Player) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let player_list = document.get_element_by_id("gameStatusRoomPlayers")
            .unwrap();
        
        let new_player = document.create_element("tr")?;

        let mut stone_color = "Black";

        if player.stone == Stone::White {
            stone_color = "White";
        }

        new_player.set_inner_html(&format!("
            <th>{}</th>
            <th>{}</th>
        ", player.username, stone_color));

        new_player.set_id(&format!("player-{}", player.username));

        player_list.append_child(&new_player)?;

        Ok(())
    }

    fn on_player_remove(username: String) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let player = document.get_element_by_id(&format!("player-{}", username))
            .unwrap();
        player.remove();

        Ok(())
    }

    fn on_player_already_logged_in() -> JsError {
        Ok(())
    }

    fn login_form_reset() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let login_username = document.get_element_by_id("loginUsername")
            .unwrap();
        let login_room = document.get_element_by_id("loginRoom")
            .unwrap();

        login_username.set_class_name("form-control");
        login_room.set_class_name("form-control");

        if let Some(login_username_error) = document.get_element_by_id("loginUsernameError") {
            login_username_error.remove();
        }

        if let Some(login_room_error) = document.get_element_by_id("loginRoomError") {
            login_room_error.remove();
        }

        Ok(())
    }

    fn status_bar_size() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let board_status = document.get_element_by_id("gameStatus")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let game_status_room = document.get_element_by_id("gameStatusRoom")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let game_status_chat = document.get_element_by_id("gameStatusChat")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let board_size = *BOARD_SIZE.lock()
            .unwrap()
            .get_mut();
        let status_height = (board_size as f64 * 9_f64 / 10_f64) as u32;
        let status_width = (board_size as f64 / 3_f64) as u32;

        board_status.style()
            .set_property("width", &format!("{}px", status_width))?;
        let partial_height = (status_height as f64 * 9_f64 / 20_f64) as u32;
        game_status_room.style()
            .set_property("height", &format!("{}px", partial_height))?;
        game_status_chat.style()
            .set_property("height", &format!("{}px", partial_height))?;

        let pass_button = document.get_element_by_id("playPass")
            .unwrap()
            .dyn_into::<HtmlElement>()?;
        let quit_button = document.get_element_by_id("playQuit")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let chat_box = document.get_element_by_id("gameStatusChatMessages")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let chat_height = (partial_height as f64 * (10_f64 / 15_f64)) as u32;

        let game_status_chat_input = document.get_element_by_id("gameStatusChatInput")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        game_status_chat_input.style()
            .set_property("margin-left", &format!("8%"))?;

        pass_button.style()
            .set_property("margin-top", &format!("8vh"))?;
        quit_button.style()
            .set_property("margin-top", &format!("8vh"))?;
        
        chat_box.style()
            .set_property("height", &format!("{}px", chat_height))?;

        Ok(())
    }

    fn status_bar_header(room_name: String) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let players = STATE.lock()
            .unwrap()
            .borrow()
            .room
            .as_ref()
            .unwrap()
            .players
            .clone();

        let status_bar_title = document.get_element_by_id("gameStatusRoomTitle")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        status_bar_title.set_inner_text(&format!("Room {}", room_name));

        let player_list = document.get_element_by_id("gameStatusRoomPlayers")
            .unwrap();
        
        for (username, val) in players.iter() {
            let player = document.create_element("tr")?;

            let mut stone_color = "Black";

            if val.stone == Stone::White {
                stone_color = "White";
            }

            player.set_inner_html(&format!("
                <th>{}</th>
                <th>{}</th>
            ", username, stone_color));

            player.set_id(&format!("player-{}", username));

            player_list.append_child(&player)?;
        }

        Ok(())
    }

    fn get_piece_position(x_i: i32, y_i: i32, inner_begin: f64, inner_size: f64, line_space: f64) -> Option<Position<u32>> {
        if x_i < (inner_begin - 4_f64 * inner_begin / 9_f64) as i32
        || x_i > (inner_begin + inner_size + 1_f64 * inner_begin / 18_f64) as i32
        || y_i < (inner_begin - 4_f64 * inner_begin / 9_f64) as i32
        || y_i > (inner_begin + inner_size + 1_f64 * inner_begin / 18_f64) as i32
        {
            return None;
        } else {
            let x = x_i as f64 - inner_begin + line_space / 2_f64;
            let y = y_i as f64 - inner_begin + line_space / 2_f64;
            let p_x = x / line_space;
            let p_y = y / line_space;

            return Some(Position(p_x as u32, p_y as u32));
        }
    }

    fn on_mouse_click(x: i32, y: i32) -> JsError {
        let mut playing = true;

        if let None = STATE.lock()
            .unwrap()
            .borrow()
            .line_space
            .as_ref()
        {
            playing = false;
        }

        if playing {
            let line_space = STATE.lock()
                .unwrap()
                .borrow()
                .line_space
                .unwrap();

            let inner_size = STATE.lock()
                .unwrap()
                .borrow()
                .inner_size
                .unwrap();

            let inner_begin = STATE.lock()
                .unwrap()
                .borrow()
                .inner_begin
                .unwrap();

            if let Some(position) = Game::get_piece_position(x, y, inner_begin, inner_size, line_space) {
                let message = ClientMessage::Place(Some(Position(position.x() as u8, position.y() as u8)));
                Game::ws_send_message(&message)?;
            }
        }

        Ok(())
    }

    fn on_next_turn(username: String) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .room
            .as_mut()
            .unwrap()
            .current_player = username.clone();

        let player_table = document.get_element_by_id("gameStatusRoomPlayers")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let players = player_table.children();

        for player_i in 0..players.length() {
            let player = players.item(player_i)
                .unwrap()
                .dyn_into::<HtmlElement>()?;

            player.style()
                .set_property("background-color", "#ffffff")?;
        }

        let player = document.get_element_by_id(&format!("player-{}", username))
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        player.style()
            .set_property("background-color", "grey")?;

        Ok(())
    }

    fn on_chat_message_received(message: String) -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let chat_area = document.get_element_by_id("gameStatusChatMessages")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let message = message.replace("<", "&lt;");
        let message = message.replace(">", "&gt;");

        let new_message = document.create_element("div")?
            .dyn_into::<HtmlElement>()?;

        new_message.set_inner_html(&(message + "<br>"));

        chat_area.append_child(&new_message)?;

        chat_area.set_scroll_top(chat_area.scroll_height());

        Ok(())
    }

    fn on_mouse_out() -> JsError {
        PREVIEW.lock()
            .unwrap()
            .borrow_mut()
            .0 = 0;
        PREVIEW.lock()
            .unwrap()
            .borrow_mut()
            .1 = 0;

        Ok(())
    }

    fn on_mouse_move(x: i32, y: i32) -> JsError {
        PREVIEW.lock()
            .unwrap()
            .borrow_mut()
            .0 = x;
        PREVIEW.lock()
            .unwrap()
            .borrow_mut()
            .1 = y;

        Ok(())
    }

    fn set_quit_button() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let quit_button = document.get_element_by_id("playQuit")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let on_quit = Closure::wrap(Box::new(|| {
            STATE.lock()
                .unwrap()
                .borrow()
                .ws
                .as_ref()
                .unwrap()
                .close()?;

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        quit_button.set_onclick(Some(on_quit.as_ref().unchecked_ref()));

        on_quit.forget();
        
        Ok(())
    }

    fn set_pass_button() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let pass_button = document.get_element_by_id("playPass")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let on_pass = Closure::wrap(Box::new(|| {
            let message = ClientMessage::Place(None);
            Game::ws_send_message(&message)?;

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        pass_button.set_onclick(Some(on_pass.as_ref().unchecked_ref()));

        on_pass.forget();

        Ok(())
    }

    fn set_message_button() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let message_button = document.get_element_by_id("gameStatusChatSubmit")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let on_message = Closure::wrap(Box::new(move || {
            let input_box = document.get_element_by_id("gameStatusChatInput")
                .unwrap()
                .dyn_into::<HtmlInputElement>()?;

            let message = input_box.value()
                .as_str()
                .to_string();

            if message.len() > 0 {
                let username = STATE.lock()
                    .unwrap()
                    .borrow()
                    .room
                    .as_ref()
                    .unwrap()
                    .self_player
                    .clone();

                let client_message = ClientMessage::Chat("<".to_owned() + &username + ">" + ": " + &message);
                Game::ws_send_message(&client_message)?;

                input_box.set_value("");
            }

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut() -> JsError>);

        message_button.set_onclick(Some(on_message.as_ref().unchecked_ref()));

        on_message.forget();

        Ok(())
    }

    fn set_enter_key() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        let submit_key = document.get_element_by_id("loginSubmit")
            .unwrap()
            .dyn_into::<HtmlElement>()?;
        let login = document.get_element_by_id("loginUsername")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        let room = document.get_element_by_id("loginRoom")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;
        let server = document.get_element_by_id("loginServer")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;

        let login_key_handler = Closure::wrap(Box::new(move |e: KeyboardEvent| {
            if e.key_code() == 13 {
                submit_key.click();
            }

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut(KeyboardEvent) -> JsError>);

        let chat_submit = document.get_element_by_id("gameStatusChatSubmit")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        let chat = document.get_element_by_id("gameStatusChatInput")
            .unwrap()
            .dyn_into::<HtmlInputElement>()?;

        let chat_key_handler = Closure::wrap(Box::new(move |e: KeyboardEvent| {
            if e.key_code() == 13 {
                chat_submit.click();
            }

            Ok::<(), JsValue>(())
        }) as Box<dyn FnMut(KeyboardEvent) -> JsError>);

        login.set_onkeydown(Some(login_key_handler.as_ref().unchecked_ref()));
        room.set_onkeydown(Some(login_key_handler.as_ref().unchecked_ref()));
        server.set_onkeydown(Some(login_key_handler.as_ref().unchecked_ref()));
        chat.set_onkeydown(Some(chat_key_handler.as_ref().unchecked_ref()));

        login_key_handler.forget();
        chat_key_handler.forget();
        Ok(())
    }

    fn render() -> JsError {
        let document = web_sys::window()
            .unwrap()
            .document()
            .unwrap();

        if let None = STATE.lock()
            .unwrap()
            .borrow()
            .room
            .as_ref()
        {
            return Err(JsValue::from_str("Game not initialized yet."));
        }

        let board_size = *BOARD_SIZE.lock()
            .unwrap()
            .get_mut() as f64;

        let side_length = STATE.lock()
            .unwrap()
            .borrow()
            .room
            .as_ref()
            .unwrap()
            .board
            .stones
            .len() as f64;
        
        let game_board = document.get_element_by_id("gameBoard")
            .unwrap()
            .dyn_into::<HtmlCanvasElement>()?;

        let dpr = web_sys::window()
            .unwrap()
            .device_pixel_ratio()
            .max(1_f64);

        game_board.style()
            .set_property("width", &format!("{}px", board_size))?;
        game_board.style()
            .set_property("height", &format!("{}px", board_size))?;

        game_board.set_attribute("width", &format!("{}", (board_size * dpr).ceil() as u32))?;
        game_board.set_attribute("height", &format!("{}", (board_size * dpr).ceil() as u32))?;


        let ctx = game_board.get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>()?;

        ctx.scale(dpr, dpr)?;

        let image = document.get_element_by_id("backgroundWood")
            .unwrap()
            .dyn_into::<HtmlImageElement>()?;
        ctx.draw_image_with_html_image_element_and_dw_and_dh(&image, 0_f64, 0_f64, board_size, board_size)?;

        let inner_size = 0.74_f64 * board_size;
        let inner_begin = (board_size - inner_size) / 2_f64;

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .inner_size = Some(inner_size);

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .inner_begin = Some(inner_begin);

        ctx.set_fill_style(&JsValue::from_str(&"black"));
        ctx.fill_rect(inner_begin - 1_f64, inner_begin - 1_f64, 1_f64, inner_size + 2_f64);
        ctx.fill_rect(inner_begin + inner_size, inner_begin - 1_f64, 1_f64, inner_size + 2_f64);
        ctx.fill_rect(inner_begin, inner_begin - 1_f64, inner_size, 1_f64);
        ctx.fill_rect(inner_begin, inner_begin + inner_size, inner_size, 1_f64);

        let line_space = inner_size / (side_length - 1_f64);

        STATE.lock()
            .unwrap()
            .borrow_mut()
            .line_space = Some(line_space);

        for i in 1..=(side_length - 2_f64) as usize {
            ctx.fill_rect(inner_begin + i as f64 * line_space - 1_f64, inner_begin, 1_f64, inner_size);
            ctx.fill_rect(inner_begin, inner_begin + i as f64 * line_space - 1_f64, inner_size, 1_f64);
        }

        if side_length == 9_f64 || side_length == 13_f64 {
            for y in &[1_f64, 3_f64] {
                for x in &[1_f64, 3_f64] {
                    ctx.begin_path();
                    ctx.arc(inner_begin + line_space * x * (side_length - 1_f64) / 4_f64,
                            inner_begin + line_space * y * (side_length - 1_f64) / 4_f64,
                            5_f64,
                            0_f64,
                            2_f64 * std::f64::consts::PI)?;
                    ctx.fill();
                }
            }

            ctx.begin_path();
            ctx.arc(inner_begin + line_space * (side_length - 1_f64) / 2_f64,
                    inner_begin + line_space * (side_length - 1_f64) / 2_f64,
                    5_f64,
                    0_f64,
                    2_f64 * std::f64::consts::PI)?;
            ctx.fill();
        }

        if side_length == 19_f64 {
            for y in &[1_f64, 3_f64, 5_f64] {
                for x in &[1_f64, 3_f64, 5_f64] {
                    ctx.begin_path();
                    ctx.arc(inner_begin + line_space * x * (side_length - 1_f64) / 6_f64,
                            inner_begin + line_space * y * (side_length - 1_f64) / 6_f64,
                            5_f64,
                            0_f64,
                            2_f64 * std::f64::consts::PI)?;
                    ctx.fill();
                }
            }
        }

        let mut font_size = (line_space / 1.4_f64) as u32;
        if side_length < 18_f64 {
            font_size = (font_size as f64 / 1.6_f64) as u32;
        }

        ctx.set_font(&format!("{}px sans serif", font_size));

        for i in 0..side_length as usize {
            if i+1 < 10 {
                ctx.fill_text(&format!("{}", i as u32 + 1), inner_begin - inner_begin * (7_f64 / 9_f64), inner_begin + line_space * i as f64 + 8_f64 * font_size as f64 / 20_f64)?;
                ctx.fill_text(&format!("{}", i as u32 + 1), inner_begin + inner_size + 5_f64 * inner_begin / 9_f64, inner_begin + line_space * i as f64 + 8_f64 * font_size as f64 / 20_f64)?;
            } else {
                ctx.fill_text(&format!("{}", i as u32 + 1), inner_begin - inner_begin * (7_f64 / 9_f64) - font_size as f64 / 2.9_f64, inner_begin + line_space * i as f64 + 8_f64 * font_size as f64 / 20_f64)?;
                ctx.fill_text(&format!("{}", i as u32 + 1), inner_begin + inner_size + 5_f64 * inner_begin / 9_f64 - font_size as f64 / 3_f64, inner_begin + line_space * i as f64 + 8_f64 * font_size as f64 / 20_f64)?;
            }

            let nest = (i as f64 / 26_f64).ceil().max(1_f64);

            let mut indicator = String::new();
            for _ in 0..nest as usize {
                indicator += &ALPHABET[i % 26].to_string();
            }
            
            if indicator == "I" || indicator == "J" {
                ctx.fill_text(&indicator, inner_begin + line_space * i as f64 - 7_f64 * font_size as f64 / 20_f64 + font_size as f64 / 4.7_f64, 4_f64 * inner_begin / 9_f64)?;
                ctx.fill_text(&indicator, inner_begin + line_space * i as f64 - 7_f64 * font_size as f64 / 20_f64 + font_size as f64 / 4.7_f64, inner_begin + inner_size + 7_f64 * inner_begin / 9_f64)?;
            } else {
                ctx.fill_text(&indicator, inner_begin + line_space * i as f64 - 7_f64 * font_size as f64 / 20_f64, 4_f64 * inner_begin / 9_f64)?;
                ctx.fill_text(&indicator, inner_begin + line_space * i as f64 - 7_f64 * font_size as f64 / 20_f64, inner_begin + inner_size + 7_f64 * inner_begin / 9_f64)?;
            }
        }

        let username = STATE.lock()
            .unwrap()
            .borrow()
            .room
            .as_ref()
            .unwrap()
            .self_player
            .clone();

        let mut player: Option<Player> = None;

        for other_player in STATE.lock()
            .unwrap()
            .borrow()
            .room
            .as_ref()
            .unwrap()
            .players
            .iter()
        {
            if other_player.0 == username {
                player = Some(other_player.1.clone());
            }
        }

        let player = player.unwrap();

        for (y, row) in STATE.lock()
            .unwrap()
            .borrow()
            .room
            .as_ref()
            .unwrap()
            .board
            .stones
            .iter()
            .enumerate()
        {
            for (x, spot) in row.iter().enumerate() {
                let mouse_position = PREVIEW.lock()
                    .unwrap()
                    .borrow()
                    .clone();

                if let Some(location) = Game::get_piece_position(mouse_position.x(), mouse_position.y(), inner_begin, inner_size, line_space)
                {
                    if player.stone == Stone::Black {
                        ctx.set_fill_style(&JsValue::from_str(&"#000000"));
                        ctx.set_global_alpha(0.006_f64);
                    } else {
                        ctx.set_fill_style(&JsValue::from_str(&"#ffffff"));
                        ctx.set_global_alpha(0.012_f64);
                    }
                    ctx.begin_path();
                    ctx.arc(inner_begin + location.x() as f64 * line_space,
                            inner_begin + location.y() as f64 * line_space,
                            line_space * (4_f64 / 9_f64),
                            0_f64,
                            2_f64 * std::f64::consts::PI)?;
                    ctx.fill();
                    ctx.set_global_alpha(1_f64);
                }

                if let Some(stone) = spot {
                    if *stone == Stone::Black {
                        ctx.set_fill_style(&JsValue::from_str(&"#000000"));
                    } else {
                        ctx.set_fill_style(&JsValue::from_str(&"#ffffff"));
                    }

                    ctx.begin_path();
                    ctx.arc(inner_begin + x as f64 * line_space,
                            inner_begin + y as f64 * line_space,
                            line_space * (4_f64 / 9_f64),
                            0_f64,
                            2_f64 * std::f64::consts::PI)?;
                    ctx.fill();
                }
            }
        }

        let game = document.get_element_by_id("game")
            .unwrap()
            .dyn_into::<HtmlElement>()?;

        game.set_hidden(false);

        Ok(())
    }
}

lazy_static::lazy_static! {
    static ref STATE: Mutex<RefCell<Game>> = Mutex::new(RefCell::new(Game {
        ws: None,
        room: None,
        inner_begin: None,
        inner_size: None,
        line_space: None,
    }));

    static ref PREVIEW: Mutex<RefCell<Position<i32>>> = Mutex::new(RefCell::new(Position(0, 0)));

    static ref BOARD_SIZE: Mutex<RefCell<u32>> = Mutex::new(RefCell::new(
            Game::get_window_size()));
}

const ALPHABET: [char; 26] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M',
                              'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z'];

#[wasm_bindgen(start)]
pub fn main() -> JsError {
    console_error_panic_hook::set_once();

    Game::login()?;

    Ok(())
}
