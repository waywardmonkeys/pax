const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const CopyWebpackPlugin = require('copy-webpack-plugin');

module.exports = {
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: 'ts-loader',
        exclude: /node_modules/,
      },
      {
        test: /\.wasm$/,
        type: 'webassembly/async',
      },
      {
        test: /\.(jpe?g|svg|png|gif|ico|eot|ttf|otf|woff2?)(\?v=\d+\.\d+\.\d+)?$/i,
        type: 'asset/resource',
      },
    ],
  },

  resolve: {
    extensions: ['.tsx', '.html', '.ts', '.js', '.wasm', '.css'],
  },

  entry: './src/index.ts',

  output: {
    path: path.join(path.resolve(__dirname), 'dist'),
    filename: 'index.js',
    publicPath: '/',
  },

  plugins: [
    new HtmlWebpackPlugin({
      template: path.resolve(__dirname, 'public/index.html'),
      inject: false
    }),
    new CopyWebpackPlugin({
      patterns: [
        {
          context: path.resolve(__dirname, 'public'),
          from: '**/*',
          to: path.resolve(__dirname, 'dist'),
          globOptions: {
            ignore: [path.resolve(__dirname, 'public', 'index.html')],
          },
        },
      ],
    }),
  ],

  experiments: {
    asyncWebAssembly: true,
  },

  mode: 'production',
};