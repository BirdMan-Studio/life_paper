use crate::state::AppState;
use axum::{Json, extract::State};
use serde_json::{Value, json};

pub async fn catalog(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"data": document(&state.components, &state.elements)}))
}

fn document(
    components: &crate::domain::ComponentRegistry,
    elements: &crate::domain::ElementRegistry,
) -> Value {
    json!({
        "version": 2,
        "title": "Python API",
        "sections": [
            {"id":"entry", "title":"入口与执行", "entries":[
                {"signature":"def main(game)", "status":"available", "description":"每 tick 调用一次同步入口，返回值忽略。状态为调用开始时的只读快照，控制在 tick 统一结算。", "returns":"None", "parameters":[], "notes":["同一组件只保留最后一条控制，吸气与吸物质互相覆盖。","调用失败保留上次成功控制；成功无输出则清空控制。","实际执行量受耐久、能量、库存和容器限制；方法返回原始请求速率。"], "example":"def main(game):\n    game.log(game.me.name, game.me.position.x, game.me.position.y)"}
            ]},
            {"id":"controls", "title":"器官控制", "entries":[
                {"signature":"part.jet(material, rate, angle=0)","status":"available","description":"喷射器官 I：扣除连接身体内的指定物质，按实际喷出质量产生反向推力和偏心转矩。","returns":"请求的 rate（mg/tick）","parameters":[["material","str","已注册物质 ID"],["rate","number","0～200 mg/tick，执行向下取整，每档 20 mg"],["angle","number","相对喷射朝向 -5～5 度，10 段，每档 1 度，四舍五入，正值顺时针"]],"notes":["库存不足自动减量；空仓不产生推力。只从连接身体取料，不自动跨组件运输。","同种物质若气仓和非气仓均有，先使用气仓。气体按独立参考体积密度换算，整微升扣除，受 2 mL/tick 上限约束。","固液暂按 1.5 g/mL 游戏密度换算，喷出物加入器官所在格汤层；气体散入环境，不增加世界气体库存。","基础能耗 5 mJ/tick，可变最高 10 mJ/tick。能量不足及耐久下降同比减少实际物质消耗与推力。"],"example":"def main(game):\n    for part in game.me.components.values():\n        if part.component == 'jet_organ_1':\n            part.jet('water', 100, angle=3)"},
                {"signature":"part.absorb_air(rate)","status":"available","description":"吸收器官 I：吸入 21% 氧气、79% 氮气，存入连接身体气仓，不扣世界库存。","returns":"请求的 rate（mL/tick）","parameters":[["rate","number","0～5 mL/tick，执行向下取整，每档 0.5 mL"]],"notes":["按器官坐标判断地层；深水、墙、悬崖禁用。","受 50 mg/tick、5 mL/tick、耐久、能量和剩余容量限制。基础 3 mJ，可变最高 5 mJ。"],"example":"absorber.absorb_air(5)"},
                {"signature":"part.absorb_matter(rate)","status":"available","description":"吸收器官 I：以器官为圆心、半径 5 px，按范围平均组成吸入并扣除世界库存。","returns":"请求的 rate（mg/tick）","parameters":[["rate","number","0～50 mg/tick，执行向下取整，每档 5 mg"]],"notes":["以范围总库存为可吸上限，按各格剩余量同比扣除。","写入连接身体非气体容器，保留小数 mg；受耐久、能量、剩余容量及 1 mL/tick 上限约束。"],"example":"absorber.absorb_matter(50)"},
                {"signature":"part.get_vision(angles=None)","status":"planned","description":"感光器官接口预留，当前调用抛出 NotImplementedError。","returns":"尚未实现","parameters":[["angles","list | None","1～120 个 -30～30 度相对角度"]],"notes":[],"example":"# 尚未开放"}
            ]},
            {"id":"self", "title":"自身与容器", "entries":[
                {"signature":"game.me","status":"available","description":"当前实例只读信息。","returns":"只读对象","parameters":[],"notes":["id、model_id、name、position.x/y、rotation（度）、rotation_mdeg、composition。","motion 可为 None，否则包含 x、y、angle_rad、velocity_x/y 和 angular_velocity。","components 为组件 UUID 到组件对象的映射。组件可读 component、placement、durability、energy_mj、properties、api、container。","容器 non_gas_contents_mg 保存各物质质量（mg）；gas_contents_ul 保存气体体积（微升）。不能直接修改快照。"],"example":"for part in game.me.components.values():\n    game.log(part.component, part.container.non_gas_contents_mg)"}
            ]},
            {"id":"world", "title":"世界查询", "entries":[
                {"signature":"game.world.get_tile(x, y)","status":"available","description":"读取一个整数坐标地格，重复查询在本轮缓存。","returns":"x、y、terrain、height、moisture、nutrients、temperature、matter_mg","parameters":[["x, y","int","有符号 32 位坐标"]],"notes":["普通模式仅可查询自身所在格；教程和简单模式可查全图。","matter_mg 为各物质当前剩余质量映射。越权抛出 PermissionError。"],"example":"tile = game.world.get_tile(game.me.position.x, game.me.position.y)\ngame.log(tile.matter_mg)"},
                {"signature":"game.world.get_tiles(points)","status":"available","description":"批量查询，保留顺序和重复项。","returns":"地格对象 tuple","parameters":[["points","list[tuple[int, int]]","1～256 个坐标"]],"notes":["任一格不可见时整批失败；与器官控制共享查询预算。"],"example":"tiles = game.world.get_tiles([(0, 0), (1, 0)])"},
                {"signature":"game.world.get_organisms(offset=0, limit=100)","status":"available","description":"分页查询可见生物概要，不返回其他实例内部状态。","returns":"含 id、name、position、rotation 的 tuple","parameters":[["offset","int","≥0"],["limit","int","1～100"]],"notes":["普通模式目前仅返回自身；可读 game.world.capabilities.full_organisms 检查权限。"],"example":"organisms = game.world.get_organisms()"}
            ]},
            {"id":"memory", "title":"记忆与日志", "entries":[
                {"signature":"game.memory","status":"available","description":"可跨 tick 持久化的实例私有 dict，成功执行后提交。","returns":"dict","parameters":[],"notes":["字符串键、可 JSON 序列化值，最多 16 KiB；禁止 NaN/Infinity。","模块全局变量不跨调用保留；game.time.step 为 AI 调用序号，不是世界 tick。"],"example":"game.memory['count'] = game.memory.get('count', 0) + 1"},
                {"signature":"game.log(*values)","status":"available","description":"写入实例运行日志，print 也可用。","returns":"None","parameters":[["values","any","任意可转成字符串的值"]],"notes":["日志最多 16384 字符。"],"example":"game.log('energy', sum(p.energy_mj for p in game.me.components.values()))"}
            ]}
        ],
        "components": components.iter().collect::<Vec<_>>(),
        "substances": elements.iter().collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn document_contains_current_catalog_and_jet_contract() {
        let value = super::document(
            &crate::domain::create_default_component_registry().unwrap(),
            &crate::domain::create_default_element_registry().unwrap(),
        );
        assert_eq!(value["substances"].as_array().unwrap().len(), 19);
        assert_eq!(
            value["sections"][1]["entries"][0]["signature"],
            "part.jet(material, rate, angle=0)"
        );
        assert_eq!(value["sections"][1]["entries"][3]["status"], "planned");
    }
}
