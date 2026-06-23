const { getDefaultConfig } = require("expo/metro-config");

module.exports = (() => {
  const config = getDefaultConfig(__dirname);

  config.resolver.extraNodeModules = {
    ...config.resolver.extraNodeModules,
    crypto: require.resolve("react-native-quick-crypto"),
    stream: require.resolve("readable-stream"),
    url: require.resolve("react-native-url-polyfill"),
    events: require.resolve("events"),
    https: require.resolve("https-browserify"),
    http: require.resolve("stream-http"),
  };

  const { transformer, resolver } = config;

  config.transformer = {
    ...transformer,
    babelTransformerPath: require.resolve("react-native-svg-transformer"),
    getTransformOptions: async () => ({
      transform: { inlineRequires: true },
    }),
  };
  config.resolver = {
    ...resolver,
    assetExts: resolver.assetExts.filter((ext) => ext !== "svg"),
    sourceExts: [...resolver.sourceExts, "svg"],
  };

  return config;
})();
