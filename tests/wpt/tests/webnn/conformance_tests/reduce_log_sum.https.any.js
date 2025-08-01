// META: title=test WebNN API reduction operations
// META: global=window
// META: variant=?cpu
// META: variant=?gpu
// META: variant=?npu
// META: script=../resources/utils.js
// META: timeout=long

'use strict';

// https://www.w3.org/TR/webnn/#dom-mlgraphbuilder-reducelogsum
// Reduce the input tensor along all dimensions, or along the axes specified in
// the axes array parameter.
//
// dictionary MLReduceOptions {
//   sequence<[EnforceRange] unsigned long> axes;
//   boolean keepDimensions = false;
// };
//
// MLOperand reduceLogSum(MLOperand input, optional MLReduceOptions options
// = {});

const reduceLogSumTests = [
  {
    'name': 'reduceLogSum float32 0D constant tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [64.54827117919922],
          'descriptor': {shape: [], dataType: 'float32'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 4.167413234710693,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 0D constant tensor empty axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [64.54827117919922],
          'descriptor': {shape: [], dataType: 'float32'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': []}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 4.167413234710693,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 1D constant tensor empty axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [64.54827117919922, 64.54827117919922],
          'descriptor': {shape: [2], dataType: 'float32'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': []}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [4.167413234710693, 4.167413234710693],
          'descriptor': {shape: [2], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float32 1D constant tensor all non-negative default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [24], dataType: 'float32'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 1D tensor all non-negative default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [24], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float32 1D tensor all non-negative integers default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            63, 82, 49, 23, 98, 67, 15, 9,  89, 7, 69, 61,
            47, 50, 41, 39, 58, 52, 35, 83, 81, 7, 34, 9
          ],
          'descriptor': {shape: [24], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.063048362731934,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 2D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [4, 6], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 3D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 4D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 5D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 1, 4, 1, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 3D tensor options.axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': [2]}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [
            5.593751907348633, 4.773046016693115, 5.3115739822387695,
            5.2497639656066895, 4.973392486572266, 5.373587131500244
          ],
          'descriptor': {shape: [2, 3], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 4D tensor options.axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': [0, 2]}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [
            5.410027980804443, 5.367736339569092, 5.399682998657227,
            4.652334213256836, 4.744638442993164, 5.565346717834473
          ],
          'descriptor': {shape: [2, 3], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 3D tensor options.keepDimensions=false',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': false}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 3D tensor options.keepDimensions=true',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': true}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.039101600646973],
          'descriptor': {shape: [1, 1, 1], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 4D tensor options.keepDimensions=false',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': false}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': 7.039101600646973,
          'descriptor': {shape: [], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float32 4D tensor options.keepDimensions=true',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': true}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.039101600646973],
          'descriptor': {shape: [1, 1, 1, 1], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float32 4D tensor options.axes with options.keepDimensions=false',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'},
          {'options': {'axes': [1, 3], 'keepDimensions': false}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [
            5.7273993492126465, 5.64375114440918, 5.453810214996338,
            5.758983135223389
          ],
          'descriptor': {shape: [2, 2], dataType: 'float32'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float32 4D tensor options.axes with options.keepDimensions=true',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.54827117919922,  97.87423706054688,  26.529027938842773,
            79.79046630859375,  50.394989013671875, 14.578407287597656,
            20.866817474365234, 32.43873596191406,  64.91233825683594,
            71.54029846191406,  11.137068748474121, 55.079307556152344,
            43.791351318359375, 13.831947326660156, 97.39019775390625,
            35.507755279541016, 52.27586364746094,  82.83865356445312,
            8.568099021911621,  0.8337112069129944, 69.23146057128906,
            3.8541641235351562, 70.5567398071289,   71.99264526367188
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float32'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'},
          {'options': {'axes': [1, 3], 'keepDimensions': true}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [
            5.7273993492126465, 5.64375114440918, 5.453810214996338,
            5.758983135223389
          ],
          'descriptor': {shape: [2, 1, 2, 1], dataType: 'float32'}
        }
      }
    }
  },

  // float16 tests
  {
    'name': 'reduceLogSum float16 0D constant tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [64.5625],
          'descriptor': {shape: [], dataType: 'float16'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [4.16796875],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 0D constant tensor empty axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [64.5625],
          'descriptor': {shape: [], dataType: 'float16'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': []}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [4.16796875],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float16 1D constant tensor all non-negative default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [24], dataType: 'float16'},
          'constant': true
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 1D tensor all non-negative default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [24], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float16 1D tensor all non-negative integers default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            63, 82, 49, 23, 98, 67, 15, 9,  89, 7, 69, 61,
            47, 50, 41, 39, 58, 52, 35, 83, 81, 7, 34, 9
          ],
          'descriptor': {shape: [24], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput':
            {'data': [7.0625], 'descriptor': {shape: [], dataType: 'float16'}}
      }
    }
  },
  {
    'name': 'reduceLogSum float16 2D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [4, 6], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 3D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 4D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 5D tensor default options',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 1, 4, 1, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [{'input': 'reduceLogSumInput'}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 3D tensor options.axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': [2]}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [5.59375, 4.7734375, 5.3125, 5.25, 4.97265625, 5.375],
          'descriptor': {shape: [2, 3], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 4D tensor options.axes',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments':
            [{'input': 'reduceLogSumInput'}, {'options': {'axes': [0, 2]}}],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [
            5.41015625, 5.3671875, 5.3984375, 4.65234375, 4.74609375, 5.56640625
          ],
          'descriptor': {shape: [2, 3], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 3D tensor options.keepDimensions=false',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': false}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 3D tensor options.keepDimensions=true',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 3, 4], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': true}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [1, 1, 1], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 4D tensor options.keepDimensions=false',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': false}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name': 'reduceLogSum float16 4D tensor options.keepDimensions=true',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'}, {'options': {'keepDimensions': true}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [7.0390625],
          'descriptor': {shape: [1, 1, 1, 1], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float16 4D tensor options.axes with options.keepDimensions=false',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'},
          {'options': {'axes': [1, 3], 'keepDimensions': false}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [5.7265625, 5.64453125, 5.453125, 5.7578125],
          'descriptor': {shape: [2, 2], dataType: 'float16'}
        }
      }
    }
  },
  {
    'name':
        'reduceLogSum float16 4D tensor options.axes with options.keepDimensions=true',
    'graph': {
      'inputs': {
        'reduceLogSumInput': {
          'data': [
            64.5625,   97.875,      26.53125, 79.8125,   50.40625,
            14.578125, 20.859375,   32.4375,  64.9375,   71.5625,
            11.140625, 55.09375,    43.78125, 13.828125, 97.375,
            35.5,      52.28125,    82.8125,  8.5703125, 0.83349609375,
            69.25,     3.853515625, 70.5625,  72
          ],
          'descriptor': {shape: [2, 2, 2, 3], dataType: 'float16'}
        }
      },
      'operators': [{
        'name': 'reduceLogSum',
        'arguments': [
          {'input': 'reduceLogSumInput'},
          {'options': {'axes': [1, 3], 'keepDimensions': true}}
        ],
        'outputs': 'reduceLogSumOutput'
      }],
      'expectedOutputs': {
        'reduceLogSumOutput': {
          'data': [5.7265625, 5.64453125, 5.453125, 5.7578125],
          'descriptor': {shape: [2, 1, 2, 1], dataType: 'float16'}
        }
      }
    }
  }
];

if (navigator.ml) {
  reduceLogSumTests.filter(isTargetTest).forEach((test) => {
    webnn_conformance_test(buildAndExecuteGraph, getPrecisionTolerance, test);
  });
} else {
  test(() => assert_implements(navigator.ml, 'missing navigator.ml'));
}
