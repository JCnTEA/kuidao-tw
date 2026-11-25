// 偏移由島仔換算，時鐘在宿主裡逐秒走動。
function refresh({settings, zones, now}) {
    const cities = JSON.parse(settings.cities);
    const clock = [];
    for (const city of cities) {
        const offset = zones?.[city.zone.trim()]?.offset_minutes;
        if (!Number.isInteger(offset)) continue;
        const name = city.label.replace(/[\x00-\x1f\x7f-\x9f]/g, "").trim() || city.zone.trim();
        clock.push({label:[...name].slice(0, 28).join(""), utc_offset_minutes:offset, format:"HH:mm:ss"});
    }
    const missing = cities.length - clock.length;
    const summary = !cities.length ? "請在插件設置中添加城市和時區"
        : missing ? `${missing} 個時區無法識別，請檢查 IANA 時區名稱` : undefined;
    const nextHour = (Math.floor(now / 3600000) + 1) * 3600;
    return {
        status:{state:"idle", label:"世界時鐘", summary, clock},
        next_refresh_at:clock.length ? nextHour : undefined
    };
}
