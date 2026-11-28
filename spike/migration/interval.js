// 宿主將日期差作為合成的分鐘輸入，驗證遷移後的區間和持久化。
function refresh(target, today) {
    const previous = lastDays();
    saveDays(Math.max(5, Math.min(180, target - today)));
    return previous;
}
