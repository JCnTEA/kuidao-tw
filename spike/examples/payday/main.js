function refresh({ settings, today }) {
    const days = Math.round((Date.parse(settings.target) - Date.parse(today)) / 86400000);
    const card = days > 0 ? `距離${settings.title}還有 ${days} 天`
        : days === 0 ? `今天是${settings.title}日` : `${settings.title}已過 ${-days} 天`;
    return { card, storage: { lastDays: days } };
}
