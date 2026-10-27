//! 仅允许 CI 在私有 Xvfb 服务中显式执行这些行为测试。
use super::*;
use std::time::{Duration, Instant};
use x11rb::protocol::{Event, xproto::*};

fn isolated_server() {
    assert_eq!(std::env::var("CI").as_deref(), Ok("true"));
    assert_eq!(
        std::env::var("ELEGANT_X11_TEST_SERVER").as_deref(),
        Ok("private-xvfb")
    );
}

fn window(x: &X11) -> u32 {
    let id = x.connection.generate_id().unwrap();
    x.connection
        .create_window(
            0,
            id,
            x.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap()
        .check()
        .unwrap();
    id
}

#[test]
#[ignore = "Run only in CI with ELEGANT_X11_TEST_SERVER=private-xvfb under xvfb-run"]
fn x11_consumers_keep_events_separate_and_release_owned_resources() {
    isolated_server();
    let query = X11::connect().unwrap();
    let main_hotkey = X11::connect_events().unwrap();
    let paste_hotkeys = X11::connect_events().unwrap();
    let outside_click = X11::connect_events().unwrap();
    let xdnd = X11::connect_events().unwrap();
    let listeners = [&main_hotkey, &paste_hotkeys, &outside_click, &xdnd];
    let windows: Vec<_> = listeners.iter().map(|x| window(x)).collect();
    let marker = query.atom("_ELEGANT_PRIVATE_EVENT_TEST").unwrap();
    for (index, id) in windows.iter().copied().enumerate() {
        query
            .connection
            .send_event(
                false,
                id,
                EventMask::NO_EVENT,
                ClientMessageEvent::new(32, id, marker, [index as u32, 0, 0, 0, 0]),
            )
            .unwrap()
            .check()
            .unwrap();
    }
    for (index, x) in listeners.iter().enumerate() {
        let deadline = Instant::now() + Duration::from_secs(1);
        let received = loop {
            if let Some(Event::ClientMessage(event)) = x.connection.poll_for_event().unwrap()
                && event.type_ == marker
            {
                break event.data.as_data32()[0];
            }
            assert!(Instant::now() < deadline, "listener lost its own event");
            std::thread::yield_now();
        };
        assert_eq!(received, index as u32);
        assert!(x.connection.poll_for_event().unwrap().is_none());
    }
    assert!(query.connection.poll_for_event().unwrap().is_none());

    // 只使用测试专有 selection，绝不接触 CLIPBOARD 或 PRIMARY。
    let selection = query.atom("_ELEGANT_PRIVATE_OWNER_TEST").unwrap();
    xdnd.connection
        .set_selection_owner(windows[3], selection, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        query
            .connection
            .get_selection_owner(selection)
            .unwrap()
            .reply()
            .unwrap()
            .owner,
        windows[3]
    );
    // 查询缓存和其他消费者仍连接时，结束拖拽也必须销毁其 owner。
    drop(xdnd);
    let deadline = Instant::now() + Duration::from_secs(1);
    while query
        .connection
        .get_selection_owner(selection)
        .unwrap()
        .reply()
        .unwrap()
        .owner
        != 0
    {
        assert!(
            Instant::now() < deadline,
            "dropped connection retained its selection"
        );
        std::thread::yield_now();
    }
    assert!(
        query
            .connection
            .get_window_attributes(windows[3])
            .unwrap()
            .reply()
            .is_err()
    );
    assert!(
        query
            .connection
            .get_window_attributes(windows[0])
            .unwrap()
            .reply()
            .is_ok()
    );

    // 缓存查询仍存活时，监听器退出也必须释放其 grab。
    let key = main_hotkey.connection.setup().min_keycode;
    main_hotkey
        .connection
        .grab_key(
            false,
            main_hotkey.root,
            ModMask::ANY,
            key,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
        .unwrap();
    assert!(
        paste_hotkeys
            .connection
            .grab_key(
                false,
                paste_hotkeys.root,
                ModMask::ANY,
                key,
                GrabMode::ASYNC,
                GrabMode::ASYNC
            )
            .unwrap()
            .check()
            .is_err()
    );
    drop(main_hotkey);
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if paste_hotkeys
            .connection
            .grab_key(
                false,
                paste_hotkeys.root,
                ModMask::ANY,
                key,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )
            .unwrap()
            .check()
            .is_ok()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "dropped listener retained its grab"
        );
        std::thread::yield_now();
    }
}
