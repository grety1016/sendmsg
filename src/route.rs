use chrono::format;
use core::panic;
use std::{borrow::Borrow, io, path::Path, result::Result, sync::Arc, time::Duration};
use tracing::field;
//引入rocket
use rocket::{
    self, build, catch,
    config::Config,
    data::{self, Data, FromData, ToByteUnit},
    fairing::AdHoc,
    form::{self, Form},
    fs::{relative, TempFile},
    futures::{SinkExt, StreamExt},
    get,
    http::Status,
    launch, outcome, post,
    request::{FromRequest, Outcome},
    response::{
        status,
        stream::{Event, EventStream},
    },
    routes,
    serde::json::Json,
    tokio::sync::broadcast::{Receiver, Sender},
    FromForm, Request, Response, Shutdown, State,
};
//引入rocket_ws
use rocket_ws::{self, stream::DuplexStream, Message, WebSocket};
//引入tokio
use rocket::tokio::{self, select, task, time};
//引入serde_json
use serde::{de::value::CowStrDeserializer, Deserialize, Serialize};
use serde_json::json; //用于结构体上方的系列化宏

//日志跟踪
pub use tracing::{event, info, trace, warn, Level};

//引入mssql
use mssql::*;
//引入seq-obj-id
use seqid::*;
//引入全局变量
// use crate::IS_WORKING;

// use either::*;

pub mod route_method;
use route_method::*;
//随机生成数字
use rand::Rng;

//引入sendmsg模块

use crate::sendmsg::*;

#[derive(FromForm, Debug)]
pub struct Upload<'r> {
    files: Vec<TempFile<'r>>,
}
pub struct Files<'r> {
    pub file_name: String,
    pub file_list: Vec<TempFile<'r>>,
}
#[post("/upload", format = "multipart/form-data", data = "<form>")]
pub async fn upload(mut form: Form<Upload<'_>>) {
    // let result = form.files.persist_to("D:/public/trf.txt").await;
    // println!("{:#?}",result);

    for file in form.files.iter_mut() {
        println!("file's name:{:#?}", file.name());
        println!(
            "file's name:{:#?}",
            file.content_type().unwrap().to_string()
        );
    }
}
//websocket connection
#[get("/ws")]
pub async fn ws(ws: WebSocket, tx: &State<Sender<String>>) -> rocket_ws::Channel<'static> {
    let mut rx = tx.subscribe();
    ws.channel(move |mut stream| {
        Box::pin(async move {
            // let mut _stream_clone = &stream;
            loop {
                select! {
                    //等待接收前端消息来执行事件函数
                   Some(msg) = stream.next() =>{
                        match msg {
                            Ok(msg) => {
                                handle_message(&mut stream,msg).await?;
                            },
                            Err(e)=> info!("{}", e),
                        }
                   }
                   //后端事件执行后触发消息机制响应
                   msg = rx.recv() => {
                    match msg {
                    Ok(msg) => {
                        stream.send(msg.into()).await?;
                    },

                    Err(e)=> info!("{}", e),
                    }

                   }
                }
            }
        })
    })
}
//如下函数用于执行接收消息后的处理函数
async fn handle_message(
    stream: &mut DuplexStream,
    msg: Message,
) -> Result<(), rocket_ws::result::Error> {
    stream.send(msg).await?;
    Ok(())
}

//SSE 连接
#[get("/event_conn")]
pub async fn event_conn() -> EventStream![] {
    println!("event_conn");
    let mut num = 0;
    EventStream! {
        loop{
            time::sleep(Duration::from_secs(1)).await;
            num+=1;
            yield Event::data(format!("form server message{}",num));
        }
    }
}

#[get("/getsmscode?<userphone>")]
pub async fn getSmsCode(userphone: String, pools: &State<Pool>) -> Json<LoginResponse> {
    let mut code = StatusCode::Success as i32;
    let mut errMsg = "".to_owned();
    let mut smsCode = 0;
    let conn = pools.get().await.unwrap();
    //查询当前手机是否在消息用户列表中存在有效验证码
    let result = conn.query_scalar_i32(sql_bind!("SELECT  DATEDIFF(second, createdtime, GETDATE())  FROM dbo.sendMsg_users WHERE userPhone = @p1", &userphone)).await.unwrap();
    //存在后判断最近一次发送时长是否在60秒内
    if let Some(val) = result {
        if val <= 60 {
            code = StatusCode::TooManyRequests as i32;
            errMsg = "操作过于频繁，请复制最近一次验证码或一分钟后重试".to_owned();
        } else {
            let mut rng = rand::thread_rng();
            smsCode = rng.gen_range(1000..10000);
        }
    } else {
        errMsg = "该手机号未注册!".to_owned();
        code = StatusCode::NotFound as i32;
    }
    //如果用户存在并在60秒内未发送验证码，则发送验证码
    if code == StatusCode::Success as i32 {
        let mut smscode :Vec<SmsMessage> = conn.query_collect(sql_bind!("UPDATE dbo.sendMsg_users SET smsCode = @p1,createdtime = getdate() WHERE userPhone = @p2
        SELECT  '' as ddtoken,dduserid,userphone,robotcode,smscode   FROM sendMsg_users  WITH(NOLOCK)  WHERE userphone = @P2
        ",smsCode,&userphone)).await.unwrap();

        if smscode[0].get_rotobotcode() == "dingrw2omtorwpetxqop" {
            let gzym_ddtoken = DDToken::new(
                "https://oapi.dingtalk.com/gettoken",
                "dingrw2omtorwpetxqop",
                "Bcrn5u6p5pQg7RvLDuCP71VjIF4ZxuEBEO6kMiwZMKXXZ5AxQl_I_9iJD0u4EQ-N",
            );
            smscode[0].set_ddtoken(gzym_ddtoken.get_token().await);
        } else {
            let zb_ddtoken = DDToken::new(
                "https://oapi.dingtalk.com/gettoken",
                "dingzblrl7qs6pkygqcn",
                "26GGYRR_UD1VpHxDBYVixYvxbPGDBsY5lUB8DcRqpSgO4zZax427woZTmmODX4oU",
            );
            smscode[0].set_ddtoken(zb_ddtoken.get_token().await);
        }
        //smscode[0].send_smsCode().await;
    }

    Json(LoginResponse {
        userPhone: userphone,
        smsCode: 0,
        token: "".to_owned(),
        code,
        errMsg,
    })
}

#[get("/shutdown")]
pub fn shutdown(_shutdown: Shutdown) -> &'static str {
    // let value = IS_WORKING.lock().unwrap();
    // if *value {
    //     "任务正在执行中,请稍后重试！"
    // } else {
    //     shutdown.notify();
    //     "优雅关机!!！"
    // }
    "优雅关机!!！"
}

#[post("/receiveMsg", format = "json", data = "<data>")]
pub async fn receiveMsg(data: Json<RecvMessage>) {
    println!("{:#?}", data);
}

#[get("/test")]
pub async fn test_fn(_pools: &State<Pool>) -> Result<Json<Content>, String> {
    // Ok(Json(Content{recognition:"Ok".into()}))
    Err("test_ERROR".into())
}

#[get("/")]
pub async fn index(pools: &State<Pool>) -> status::Custom<&'static str> {
    let conn = pools.get().await.unwrap();

    let mut result = conn
        .query("SELECT top 1 1 FROM dbo.T_SEC_USER")
        .await
        .unwrap();
    if let Some(row) = result.fetch().await.unwrap() {
        println!("server is working:{:?}!", row.try_get_i32(0).unwrap());
    }
    crate::local_thread().await;
    status::Custom(
        Status::Ok,
        "您好,欢迎使用快先森金蝶消息接口,请前往  http://8sjqkmbn.beesnat.com  访问！",
    )
}

//当用户不是从前端页面发起请求时，则返回登录页面
#[get("/login")]
pub async fn login_get() -> ApiResponse<CstResponse> {
    let errmsg =
        "您好,欢迎使用快先森金蝶消息接口,请先前往  http://8sjqkmbn.beesnat.com  登录!".to_owned();
    let cstcode = CstResponse::new(StatusCode::Unauthorized as i32, errmsg);

    ApiResponse::Unauthorized(Json(cstcode))
}

//请求守卫，用于验证表头与表体token是否匹配
#[rocket::async_trait]
impl<'r> FromRequest<'r> for LoginResponse {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let token = req
            .headers()
            .get_one("Authorization")
            .unwrap_or("")
            .to_owned();
        let userPhone = Claims::get_phone(token.to_string()).await;

        Outcome::Success(LoginResponse::new(
            token.clone(),
            LoginUser {
                userPhone,
                smsCode: "".to_owned(),
                token,
            },
            StatusCode::Success as i32,
            "".to_string(),
        ))
    }
}

#[post("/login", format = "application/json", data = "<user>")]
pub async fn login_post<'r>(
    loginrespon: LoginResponse,
    user: Json<LoginUser>,
    pools: &State<Pool>,
) -> Json<LoginResponse> {
    let Json(userp) = user;
    // assert_eq!(userp.token.is_empty(),false);
    // assert_eq!(Claims::verify_token(userp.token.clone()).await,true);
    if Claims::verify_token(loginrespon.token.clone()).await {
        // println!("token验证成功：{:#?}", &userp.token);
        Json(loginrespon)
    } else if userp.userPhone.is_empty() || userp.smsCode.is_empty() {
        // println!("用户名或密码为空：{:#?}", &userp.token);
        return Json(LoginResponse::new(
            "Bearer".to_string(),
            userp.clone(),
            StatusCode::RequestEntityNull as i32,
            "手机号或验证码不能为空!".to_string(),
        ));
    } else {
        let conn = pools.get().await.unwrap();
        //查询当前用户列表中是否存在该手机及验证码，并且在3分钟时效内
        let userPhone = conn
                .query_scalar_string(sql_bind!(
                    "SELECT  userPhone  FROM dbo.sendMsg_users WHERE userphone = @P1 AND smscode = @P2 AND   DATEDIFF(MINUTE, createdtime, GETDATE()) <= 3",
                    &userp.userPhone,
                    &userp.smsCode
                ))
                .await
                .unwrap();
        let mut token = String::from("");
        // #[allow(unused)]
        let mut code: i32 = StatusCode::Success as i32;
        let mut errmsg = String::from("");

        if let Some(value) = userPhone {
            token = Claims::get_token(value.to_owned()).await;
        } else {
            code = StatusCode::RequestEntityNotMatch as i32;
            errmsg = "手机号或验证码错误!".to_owned();
        }

        // if code == 0 {println!("创建token成功：{:#?}", &userp.token);}else{println!("用户名或密码错误!")}
        Json(LoginResponse::new(token, userp.clone(), code, errmsg))
    }
    // 加入任务
}

#[get("/getitemlist?<userphone>&<itemstatus>")]
pub async fn getItemList(
    loginrespon: LoginResponse,
    mut userphone: String,
    itemstatus: String,
    pool: &State<Pool>,
) -> Json<Vec<FlowItemList>> {
    //判断token解析出来的手机号是否与请求参数中的手机号一致，如果不一致，则使用token的手机号
    let tokenPhone = Claims::get_phone(loginrespon.token.clone()).await;
    if tokenPhone != userphone {
        userphone = tokenPhone;
    }

    let conn = pool.get().await.unwrap();
    //println!("userphone:{},itemstatus:{}", &userphone, &itemstatus);
    let flowitemlist = conn
        .query_collect(sql_bind!(
            "SELECT * FROM getTodoList(@p1,@p2)",
            &itemstatus,
            &userphone
        ))
        .await
        .unwrap();
    Json(flowitemlist)
}

#[get("/getflowdetail?<fprocinstid>&<fformtype>")]
pub async fn getFlowDetail(
    loginrespon: LoginResponse,
    fprocinstid: String,
    fformtype: String,
    pool: &State<Pool>,
) -> ApiResponse<Vec<FlowDetail>> {
    let conn = pool.get().await.unwrap();

    let flowdetail: Vec<FlowDetail> = conn
        .query_collect(sql_bind!(
            "SELECT * FROM getFlowDetail(@p1,@p2,@p3)",
            &fprocinstid,
            &loginrespon.userPhone,
            &fformtype
        ))
        .await
        .unwrap();
    if flowdetail[0].available == 1 {
        ApiResponse::Success(Json(flowdetail))
    } else {
        ApiResponse::Forbidden(Json(flowdetail))
    }
}

#[get("/getflowdetailrows?<fprocinstid>")]
pub async fn getFlowDetailRows(
    fprocinstid: String,
    pool: &State<Pool>,
) -> Json<Vec<FlowDetailRow>> {
    let conn = pool.get().await.unwrap();
    //查询明细行的数据数组
    let mut flowdetailrows: Vec<FlowDetailRow> = conn
        .query_collect(sql_bind!(
            "SELECT * FROM getFlowDetailRows(@p1)",
            &fprocinstid
        ))
        .await
        .unwrap();
    //遍历明细行数据
    for detailrow in flowdetailrows.iter_mut() {
        //将附件数据转换成json数组
        detailrow.attachments =
            serde_json::from_str(&detailrow.fSnnaAttachments).unwrap_or(Some(vec![]));
        //清空附件字符串
        detailrow.fSnnaAttachments = "".to_string();
        //遍历Optiono数据
        for Attachment in detailrow.attachments.iter_mut() {
            for item in Attachment.iter_mut() {
                let path = Path::new(item.FileName.as_str())
                    .extension()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                item.FileType = Some(path.to_string());
                let filepath = match path.as_str() {
                    "jpg" | "png" | "jpeg" | "gif" => {
                        format!(
                            "http://sendmsg.free.idcfengye.com/files/Image/{}/{}",
                            detailrow.years, item.ServerFileName
                        )
                    }
                    "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => {
                        format!(
                            "http://sendmsg.free.idcfengye.com/files/Doc/{}/{}",
                            detailrow.years, item.ServerFileName
                        )
                    }
                    _ => {
                        format!(
                            "http://sendmsg.free.idcfengye.com/files/Other/{}/{}",
                            detailrow.years, item.ServerFileName
                        )
                    } //"txt" | "rar" | "zip" | "csv"
                };
                item.ServerFileName = format!("{}.{}", filepath, path);
                if item.FileBytesLength / 1024_f64 >= 1024_f64 {
                    item.FileSize = Some(format!(
                        "{:.2}MB",
                        item.FileBytesLength / 1024_f64 / 1024_f64
                    ));
                    item.FileBytesLength = 0_f64;
                    item.FileLength = 0_f64;
                } else {
                    item.FileSize = Some(format!("{:.2}KB", item.FileBytesLength / 1024_f64));
                    item.FileBytesLength = 0_f64;
                    item.FileLength = 0_f64;
                }
            }
        }
    }
    Json(flowdetailrows)
}

#[catch(default)]
pub async fn default_catcher(status: Status, req: &Request<'_>) -> ApiResponse<CstResponse> {
    // println!("not_found:{:#?}", req);
    let mut url = req.headers().get_one("originURL").unwrap().to_string();

    #[allow(unused_assignments)]
    let mut apires = ApiResponse::NotFound(Json(CstResponse::new(
        StatusCode::NotFound as i32,
        "".to_string(),
    )));

    if Status::NotFound == status {
        url = format!(
            "您访问的地址 {} 不存在, 请检查地址(方法Get/Post)后重试!",
            url
        );
        apires = ApiResponse::NotFound(Json(CstResponse::new(StatusCode::NotFound as i32, url)));
    } else if Status::UnprocessableEntity == status {
        url = format!("您访问的地址  {} 请求参数不正确,请检查参数后重试!", url);
        apires = ApiResponse::UnprocessableEntity(Json(CstResponse::new(
            StatusCode::UnprocessableEntity as i32,
            url,
        )));
    } else if Status::BadRequest == status {
        url = format!(
            "您访问的地址  {} 缺少请求主体或不正确,请检查参数后重试!",
            url
        );
        apires = ApiResponse::UnprocessableEntity(Json(CstResponse::new(
            StatusCode::BadRequest as i32,
            url,
        )));
    } else if Status::Unauthorized == status {
        url = format!("您访问的地址  {} 未授权,请检查权限后重试!", url);
        apires =
            ApiResponse::Unauthorized(Json(CstResponse::new(StatusCode::Unauthorized as i32, url)));
    } else {
        url = format!("您访问的地址 {} 发生未知错误,请联系管理员!", url);
        apires = ApiResponse::InternalServerError(Json(CstResponse::new(
            StatusCode::InternalServerError as i32,
            url,
        )));
    }
    apires
}
