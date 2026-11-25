function refresh({ source }) {
  if (source.status === "unconfigured") return { card: "請授權 HTTPS 數據源" };
  if (source.status === "offline") return { card: "倉庫數據暫不可用" };
  const repository = JSON.parse(source.text);
  return { card: "Rust · " + repository.stargazers_count + " 星標" };
}
